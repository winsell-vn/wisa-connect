//! Turns a WISA account into configured clients.
//!
//! The app never writes a client config itself: it creates (or refreshes) a
//! `WISA Gateway` provider and switches to it, so the provider engine already in
//! this codebase does the writing — atomic replace, backup, crash recovery and
//! the per-client rules included.
//!
//! The key is the account's single `ws_live_…` key, read back from the API
//! rather than minted here. One key for every client is the point: rotating it
//! once fixes all of them, and a second machine can fetch the same one instead
//! of forcing a rotation that breaks the first.

use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::PathBuf;
use tauri::State;

use super::auth::{self, WisaAuthState};
use super::client;
use crate::app_config::AppType;
use crate::provider::Provider;
use crate::services::ProviderService;
use crate::store::AppState;

/// Stable id: provisioning twice updates this card instead of adding another.
pub const PROVIDER_ID: &str = "wisa-gateway";
const PROVIDER_NAME: &str = "WISA Gateway";

/// The clients this app knows how to point at WISA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    ClaudeCode,
    CodexCli,
    ClaudeDesktop,
}

impl Target {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "claude_code" => Some(Self::ClaudeCode),
            "codex_cli" => Some(Self::CodexCli),
            "claude_desktop_3p" => Some(Self::ClaudeDesktop),
            _ => None,
        }
    }

    pub fn id(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude_code",
            Self::CodexCli => "codex_cli",
            Self::ClaudeDesktop => "claude_desktop_3p",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::CodexCli => "Codex CLI",
            Self::ClaudeDesktop => "Claude Desktop",
        }
    }

    pub fn app_type(&self) -> AppType {
        match self {
            Self::ClaudeCode => AppType::Claude,
            Self::CodexCli => AppType::Codex,
            Self::ClaudeDesktop => AppType::ClaudeDesktop,
        }
    }

    /// Whether the client looks present on this machine. A client that is not
    /// installed is still offered: the user may be about to install it, and a
    /// config written now is waiting for it.
    pub fn installed(&self) -> bool {
        let home = crate::config::get_home_dir();
        match self {
            Self::ClaudeCode => home.join(".claude").exists(),
            Self::CodexCli => home.join(".codex").exists(),
            Self::ClaudeDesktop => claude_desktop_dir().is_some_and(|dir| dir.exists()),
        }
    }
}

fn claude_desktop_dir() -> Option<PathBuf> {
    if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(|base| PathBuf::from(base).join("Claude"))
    } else if cfg!(target_os = "macos") {
        Some(crate::config::get_home_dir().join("Library/Application Support/Claude"))
    } else {
        None
    }
}

fn all_targets() -> Vec<Target> {
    vec![Target::ClaudeCode, Target::CodexCli, Target::ClaudeDesktop]
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientStatus {
    pub target: String,
    pub label: String,
    pub installed: bool,
    pub connected: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyStatus {
    pub prefix: String,
    pub fingerprint: String,
    pub state: String,
    pub created_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WisaStatus {
    pub signed_in: bool,
    pub email: Option<String>,
    pub key: Option<KeyStatus>,
    pub clients: Vec<ClientStatus>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProvisionOutcome {
    pub target: String,
    pub label: String,
    pub ok: bool,
    pub message: Option<String>,
}

// ---------------------------------------------------------------------------
// Account key
// ---------------------------------------------------------------------------

/// The account key, or `None` when the account has never had one issued.
async fn reveal_key(state: &WisaAuthState) -> Result<Option<String>, String> {
    match get_with_retry(state, "/account/api-key/secret").await {
        Ok(value) => Ok(value
            .get("secret")
            .and_then(Value::as_str)
            .map(str::to_string)),
        Err(error) if error.is_not_found() => Ok(None),
        Err(error) => Err(error.message),
    }
}

/// `GET` with the current token, renewing once if the API says it is no longer
/// valid. The access token lives only in memory, so a window left open past its
/// lifetime would otherwise fail every call until relaunch.
async fn get_with_retry(
    state: &WisaAuthState,
    path: &str,
) -> Result<Value, client::ApiError> {
    let token = auth::access_token(state)
        .await
        .map_err(|message| client::ApiError {
            status: 401,
            message,
        })?;
    match client::get_json(path, Some(&token)).await {
        Err(error) if error.is_unauthorized() => {
            let token = auth::refresh(state)
                .await
                .map_err(|message| client::ApiError {
                    status: 401,
                    message,
                })?;
            client::get_json(path, Some(&token)).await
        }
        other => other,
    }
}

/// Issues the first key through the setup-token flow the web page already uses,
/// then reads it back so every client shares one `ws_live_…` key.
async fn issue_key(state: &WisaAuthState, target: Target) -> Result<String, String> {
    let token = auth::access_token(state).await?;
    let issued = client::post_json(
        "/client-setup/issue",
        &json!({
            "target": target.id(),
            "platform": std::env::consts::OS,
            "architecture": std::env::consts::ARCH,
        }),
        Some(&token),
    )
    .await
    .map_err(|error| error.message)?;

    let setup_token = issued
        .get("setupToken")
        .and_then(Value::as_str)
        .ok_or("WISA không trả về mã cài đặt.")?;

    let consumed = client::post_json(
        "/client-setup/consume",
        &json!({ "setupToken": setup_token }),
        None,
    )
    .await
    .map_err(|error| error.message)?;

    if let Some(secret) = reveal_key(state).await? {
        return Ok(secret);
    }

    // Account keys are off on this deployment: the setup token's credential is
    // a per-client gateway token, so it is usable for this target only.
    consumed
        .get("credential")
        .and_then(|credential| credential.get("accessToken"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| "WISA không trả về khoá truy cập.".to_string())
}

async fn ensure_key(state: &WisaAuthState, target: Target) -> Result<String, String> {
    match reveal_key(state).await? {
        Some(secret) => Ok(secret),
        None => issue_key(state, target).await,
    }
}

async fn key_status(state: &WisaAuthState) -> Option<KeyStatus> {
    let keys = get_with_retry(state, "/account/api-key").await.ok()?;
    let items = keys.as_array()?;
    let active = items
        .iter()
        .find(|item| item.get("state").and_then(Value::as_str) == Some("active"))
        .or_else(|| items.first())?;
    Some(KeyStatus {
        prefix: active
            .get("prefix")
            .and_then(Value::as_str)
            .unwrap_or("ws_live_")
            .to_string(),
        fingerprint: active
            .get("fingerprint")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        state: active
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string(),
        created_at: active
            .get("createdAt")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

// ---------------------------------------------------------------------------
// Provider construction
// ---------------------------------------------------------------------------

async fn fetch_models(key: &str) -> Vec<String> {
    let url = format!("{}/v1/models", client::gateway_base());
    match client::get_absolute(&url, Some(key)).await {
        Ok(value) => value
            .get("data")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.get("id").and_then(Value::as_str))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        // A model list is a nicety: without it the provider is still correct,
        // the client just asks the gateway for its default.
        Err(_) => Vec::new(),
    }
}

fn pick_model(models: &[String], prefixes: &[&str]) -> Option<String> {
    for prefix in prefixes {
        if let Some(found) = models
            .iter()
            .find(|id| id.to_ascii_lowercase().starts_with(prefix))
        {
            return Some(found.clone());
        }
    }
    models.first().cloned()
}

/// A small, cheap model for Claude Code's background calls, when the catalog
/// has one. Falls back to the main model rather than inventing an id.
fn pick_fast_model(models: &[String], main: &str) -> String {
    models
        .iter()
        .find(|id| {
            let lower = id.to_ascii_lowercase();
            lower.contains("flash") || lower.contains("mini") || lower.contains("haiku")
        })
        .cloned()
        .unwrap_or_else(|| main.to_string())
}

fn anthropic_settings(key: &str, main: &str, fast: &str) -> Value {
    let mut env = Map::new();
    env.insert(
        "ANTHROPIC_BASE_URL".into(),
        json!(client::gateway_base()),
    );
    env.insert("ANTHROPIC_AUTH_TOKEN".into(), json!(key));
    env.insert("ANTHROPIC_MODEL".into(), json!(main));
    env.insert("ANTHROPIC_DEFAULT_SONNET_MODEL".into(), json!(main));
    env.insert("ANTHROPIC_DEFAULT_OPUS_MODEL".into(), json!(main));
    env.insert("ANTHROPIC_DEFAULT_HAIKU_MODEL".into(), json!(fast));
    json!({ "env": Value::Object(env) })
}

fn codex_settings(key: &str, model: &str) -> Value {
    let model_literal = serde_json::to_string(model).unwrap_or_else(|_| "\"\"".to_string());
    let config = format!(
        "model_provider = \"wisa\"\nmodel = {model_literal}\nmodel_reasoning_effort = \"high\"\ndisable_response_storage = true\n\n[model_providers.wisa]\nname = \"{PROVIDER_NAME}\"\nbase_url = \"{base}/v1\"\nwire_api = \"responses\"\nrequires_openai_auth = true\n",
        base = client::gateway_base()
    );
    json!({
        "auth": { "OPENAI_API_KEY": key },
        "config": config,
    })
}

fn build_provider(target: Target, key: &str, models: &[String]) -> Provider {
    let settings_config = match target {
        Target::ClaudeCode => {
            let main = pick_model(models, &["claude-", "claude"])
                .unwrap_or_else(|| "claude-sonnet-4-5".to_string());
            let fast = pick_fast_model(models, &main);
            anthropic_settings(key, &main, &fast)
        }
        Target::CodexCli => {
            let model = pick_model(models, &["gpt-5", "gpt-", "o3", "o4", "codex"])
                .unwrap_or_else(|| "gpt-5".to_string());
            codex_settings(key, &model)
        }
        Target::ClaudeDesktop => {
            let main = pick_model(models, &["claude-", "claude"])
                .unwrap_or_else(|| "claude-sonnet-4-5".to_string());
            let fast = pick_fast_model(models, &main);
            anthropic_settings(key, &main, &fast)
        }
    };

    let mut provider = Provider::with_id(
        PROVIDER_ID.to_string(),
        PROVIDER_NAME.to_string(),
        settings_config,
        Some("https://wisa.vn".to_string()),
    );
    provider.category = Some("custom".to_string());
    provider.icon = Some("wisa".to_string());
    provider.icon_color = Some("#1F6FEB".to_string());
    provider.notes = Some("Tạo tự động bởi WISA Connect.".to_string());
    provider
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn status_for(state: &AppState, targets: &[Target]) -> Vec<ClientStatus> {
    targets
        .iter()
        .map(|target| {
            let app_type = target.app_type();
            let exists = state
                .db
                .get_provider_by_id(PROVIDER_ID, app_type.as_str())
                .ok()
                .flatten()
                .is_some();
            let current = ProviderService::current(state, app_type.clone()).ok();
            ClientStatus {
                target: target.id().to_string(),
                label: target.label().to_string(),
                installed: target.installed(),
                connected: exists && current.as_deref() == Some(PROVIDER_ID),
            }
        })
        .collect()
}

#[tauri::command]
pub async fn wisa_status(
    state: State<'_, AppState>,
    auth: State<'_, WisaAuthState>,
) -> Result<WisaStatus, String> {
    let session = auth::ensure_session(auth.inner()).await?;

    let app_state = state.inner().clone();
    let clients = tauri::async_runtime::spawn_blocking(move || status_for(&app_state, &all_targets()))
        .await
        .map_err(|error| format!("Không đọc được trạng thái client: {error}"))?;

    let Some(session) = session else {
        return Ok(WisaStatus {
            signed_in: false,
            email: None,
            key: None,
            clients,
        });
    };

    let email = session.email().map(str::to_string);
    // The key panel is decoration: a failed read must not turn the whole status
    // page into an error screen.
    let key = key_status(auth.inner()).await;

    Ok(WisaStatus {
        signed_in: true,
        email,
        key,
        clients,
    })
}

/// Configures the requested clients. Each one succeeds or fails on its own so a
/// single unsupported client cannot undo the others.
#[tauri::command]
pub async fn wisa_provision(
    state: State<'_, AppState>,
    auth: State<'_, WisaAuthState>,
    targets: Vec<String>,
) -> Result<Vec<ProvisionOutcome>, String> {
    let selected: Vec<Target> = if targets.is_empty() {
        all_targets()
    } else {
        targets.iter().filter_map(|id| Target::parse(id)).collect()
    };
    if selected.is_empty() {
        return Err("Không có client nào được chọn.".to_string());
    }

    let key = ensure_key(auth.inner(), selected[0]).await?;
    let models = fetch_models(&key).await;

    let providers: Vec<(Target, Provider)> = selected
        .iter()
        .map(|target| (*target, build_provider(*target, &key, &models)))
        .collect();

    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut outcomes = Vec::with_capacity(providers.len());
        for (target, provider) in providers {
            let app_type = target.app_type();
            let existing = app_state
                .db
                .get_provider_by_id(PROVIDER_ID, app_type.as_str())
                .ok()
                .flatten();

            let saved = match existing {
                Some(_) => ProviderService::update(
                    &app_state,
                    app_type.clone(),
                    Some(PROVIDER_ID),
                    provider,
                ),
                None => ProviderService::add(&app_state, app_type.clone(), provider, false),
            };

            let outcome = match saved {
                Ok(_) => match ProviderService::switch(&app_state, app_type, PROVIDER_ID) {
                    Ok(result) if result.warnings.is_empty() => ProvisionOutcome {
                        target: target.id().to_string(),
                        label: target.label().to_string(),
                        ok: true,
                        message: None,
                    },
                    Ok(result) => ProvisionOutcome {
                        target: target.id().to_string(),
                        label: target.label().to_string(),
                        ok: true,
                        message: Some(result.warnings.join(" ")),
                    },
                    Err(error) => ProvisionOutcome {
                        target: target.id().to_string(),
                        label: target.label().to_string(),
                        ok: false,
                        message: Some(error.to_string()),
                    },
                },
                Err(error) => ProvisionOutcome {
                    target: target.id().to_string(),
                    label: target.label().to_string(),
                    ok: false,
                    message: Some(error.to_string()),
                },
            };
            outcomes.push(outcome);
        }
        outcomes
    })
    .await
    .map_err(|error| format!("Cấu hình client thất bại: {error}"))
}

/// Removes the WISA provider from the given clients. Their own config files are
/// rewritten by the same engine that wrote them, so whatever was there before
/// the first install is restored from the backup the engine keeps.
#[tauri::command]
pub async fn wisa_unlink(
    state: State<'_, AppState>,
    targets: Vec<String>,
) -> Result<Vec<ProvisionOutcome>, String> {
    let selected: Vec<Target> = if targets.is_empty() {
        all_targets()
    } else {
        targets.iter().filter_map(|id| Target::parse(id)).collect()
    };

    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        selected
            .into_iter()
            .map(|target| {
                let app_type = target.app_type();
                let exists = app_state
                    .db
                    .get_provider_by_id(PROVIDER_ID, app_type.as_str())
                    .ok()
                    .flatten()
                    .is_some();
                if !exists {
                    return ProvisionOutcome {
                        target: target.id().to_string(),
                        label: target.label().to_string(),
                        ok: true,
                        message: None,
                    };
                }
                match ProviderService::delete(&app_state, app_type, PROVIDER_ID) {
                    Ok(()) => ProvisionOutcome {
                        target: target.id().to_string(),
                        label: target.label().to_string(),
                        ok: true,
                        message: None,
                    },
                    Err(error) => ProvisionOutcome {
                        target: target.id().to_string(),
                        label: target.label().to_string(),
                        ok: false,
                        message: Some(error.to_string()),
                    },
                }
            })
            .collect::<Vec<_>>()
    })
    .await
    .map_err(|error| format!("Ngắt kết nối thất bại: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_round_trip_through_their_ids() {
        for target in all_targets() {
            assert_eq!(Target::parse(target.id()), Some(target));
        }
        assert_eq!(Target::parse("gemini_cli"), None);
    }

    #[test]
    fn claude_settings_carry_the_gateway_and_the_key() {
        let settings = anthropic_settings("ws_live_x", "claude-sonnet-4-5", "claude-haiku-4-5");
        let env = settings.get("env").and_then(Value::as_object).unwrap();
        assert_eq!(
            env.get("ANTHROPIC_BASE_URL").and_then(Value::as_str),
            Some(client::gateway_base().as_str())
        );
        assert_eq!(
            env.get("ANTHROPIC_AUTH_TOKEN").and_then(Value::as_str),
            Some("ws_live_x")
        );
        assert_eq!(
            env.get("ANTHROPIC_MODEL").and_then(Value::as_str),
            Some("claude-sonnet-4-5")
        );
    }

    #[test]
    fn codex_settings_point_at_the_v1_gateway() {
        let settings = codex_settings("ws_live_x", "gpt-5");
        assert_eq!(
            settings.pointer("/auth/OPENAI_API_KEY").and_then(Value::as_str),
            Some("ws_live_x")
        );
        let config = settings.get("config").and_then(Value::as_str).unwrap();
        assert!(config.contains(&format!("base_url = \"{}/v1\"", client::gateway_base())));
        assert!(config.contains("model = \"gpt-5\""));
        assert!(config.contains("[model_providers.wisa]"));
    }

    #[test]
    fn pick_model_prefers_the_requested_family() {
        let models = vec!["gpt-5-mini".to_string(), "claude-sonnet-4-5".to_string()];
        assert_eq!(
            pick_model(&models, &["claude-"]).as_deref(),
            Some("claude-sonnet-4-5")
        );
        assert_eq!(
            pick_model(&models, &["nothing-"]).as_deref(),
            Some("gpt-5-mini")
        );
    }

    #[test]
    fn fast_model_falls_back_to_the_main_one() {
        let models = vec!["claude-sonnet-4-5".to_string()];
        assert_eq!(pick_fast_model(&models, "claude-sonnet-4-5"), "claude-sonnet-4-5");
        let with_flash = vec!["claude-sonnet-4-5".to_string(), "gemini-3-flash".to_string()];
        assert_eq!(pick_fast_model(&with_flash, "claude-sonnet-4-5"), "gemini-3-flash");
    }
}
