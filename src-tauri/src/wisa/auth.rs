//! Google sign-in for the WISA account.
//!
//! The flow is the desktop one the API already supports: an authorization URL
//! built from a PKCE challenge, opened in the user's real browser, answered on a
//! loopback listener the app is holding. Google therefore sees a normal browser
//! with the user's existing session, and the app never renders a login form and
//! never handles a password.
//!
//! `state` is checked before the code is exchanged, and the verifier never
//! leaves this process, so a code delivered to another app on the machine
//! cannot be redeemed.

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::Mutex;
use std::time::Duration;
use tauri::State;
use tauri_plugin_opener::OpenerExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::client;
use super::keychain;

/// How long the browser gets to come back before the sign-in is abandoned.
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(180);
/// A connection that never sends a request line (Chrome preconnects) is dropped
/// after this long instead of stalling the wait.
const CONNECTION_IDLE_TIMEOUT: Duration = Duration::from_secs(10);

const SESSION_FILE: &str = "wisa-session.json";

/// The signed-in session. Never serialized towards the webview: only
/// [`WisaSessionView`] is.
#[derive(Debug, Clone)]
pub struct Session {
    pub user: Value,
    pub access_token: String,
    pub refresh_token: String,
}

impl Session {
    fn from_auth_response(response: &Value) -> Result<Self, String> {
        let access_token = response
            .get("accessToken")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("WISA không trả về access token.")?
            .to_string();
        let refresh_token = response
            .get("refreshToken")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or("WISA không trả về refresh token.")?
            .to_string();
        let user = response
            .get("user")
            .cloned()
            .filter(|value| value.is_object())
            .ok_or("WISA không trả về thông tin tài khoản.")?;
        Ok(Self {
            user,
            access_token,
            refresh_token,
        })
    }

    pub fn email(&self) -> Option<&str> {
        self.user.get("email").and_then(Value::as_str)
    }
}

/// What the UI is allowed to see.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WisaSessionView {
    pub signed_in: bool,
    pub user: Option<Value>,
    pub email: Option<String>,
}

impl WisaSessionView {
    pub fn signed_out() -> Self {
        Self {
            signed_in: false,
            user: None,
            email: None,
        }
    }

    fn of(session: &Session) -> Self {
        Self {
            signed_in: true,
            user: Some(session.user.clone()),
            email: session.email().map(str::to_string),
        }
    }
}

/// Process-wide sign-in state, managed by Tauri.
#[derive(Default)]
pub struct WisaAuthState {
    session: Mutex<Option<Session>>,
    /// Serializes the first restore so two panels opening at once do not both
    /// spend the refresh token.
    restoring: tokio::sync::Mutex<()>,
}

impl WisaAuthState {
    fn cached(&self) -> Option<Session> {
        self.session.lock().ok().and_then(|slot| slot.clone())
    }

    fn store(&self, session: Option<Session>) {
        if let Ok(mut slot) = self.session.lock() {
            *slot = session;
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct PersistedSession {
    user: Value,
    #[serde(default)]
    saved_at: Option<i64>,
}

fn session_path() -> std::path::PathBuf {
    crate::config::get_app_config_dir().join(SESSION_FILE)
}

fn persist(session: &Session) -> Result<(), String> {
    keychain::save(&session.refresh_token)?;
    let path = session_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Không tạo được thư mục cấu hình: {error}"))?;
    }
    let payload = PersistedSession {
        user: session.user.clone(),
        saved_at: Some(chrono::Utc::now().timestamp()),
    };
    let text = serde_json::to_string_pretty(&payload)
        .map_err(|error| format!("Không ghi được phiên đăng nhập: {error}"))?;
    std::fs::write(&path, text).map_err(|error| format!("Không ghi được phiên đăng nhập: {error}"))
}

fn read_persisted() -> Option<PersistedSession> {
    let text = std::fs::read_to_string(session_path()).ok()?;
    serde_json::from_str(&text).ok()
}

fn forget() {
    let _ = keychain::clear();
    let _ = std::fs::remove_file(session_path());
}

/// Returns a usable session, restoring it from the keychain when the process is
/// starting fresh. `None` means the user is signed out.
pub async fn ensure_session(state: &WisaAuthState) -> Result<Option<Session>, String> {
    if let Some(session) = state.cached() {
        return Ok(Some(session));
    }
    let _guard = state.restoring.lock().await;
    if let Some(session) = state.cached() {
        return Ok(Some(session));
    }
    let Some(persisted) = read_persisted() else {
        return Ok(None);
    };
    let Some(refresh_token) = keychain::load()? else {
        return Ok(None);
    };
    match client::post_json(
        "/auth/refresh",
        &json!({ "refreshToken": refresh_token }),
        None,
    )
    .await
    {
        Ok(response) => {
            let mut session = Session::from_auth_response(&response)?;
            // The refresh endpoint may rotate the token; whatever it answered
            // with is what the next restore must use.
            persist(&session)?;
            session.user = if session.user.is_object() {
                session.user.clone()
            } else {
                persisted.user.clone()
            };
            state.store(Some(session.clone()));
            Ok(Some(session))
        }
        Err(error) if error.is_unauthorized() => {
            // The stored session is dead: clear it so the next launch does not
            // spend another round trip proving the same thing.
            forget();
            Ok(None)
        }
        Err(error) => {
            // Offline, or the API is down. The profile we already have is
            // better than an empty sign-in screen.
            if persisted.user.is_object() {
                let session = Session {
                    user: persisted.user,
                    access_token: String::new(),
                    refresh_token,
                };
                state.store(Some(session.clone()));
                return Ok(Some(session));
            }
            Err(error.message)
        }
    }
}

/// A bearer token for an account call, renewed when it has to be.
pub async fn access_token(state: &WisaAuthState) -> Result<String, String> {
    let session = ensure_session(state)
        .await?
        .ok_or("Bạn cần đăng nhập WISA trước.")?;
    if !session.access_token.is_empty() {
        return Ok(session.access_token);
    }
    renew(state, &session.refresh_token).await
}

/// Exchanges the refresh token for a new access token.
///
/// A rejected refresh token means the sign-in is over — the session is dropped
/// so the gate shows the sign-in screen instead of an app that fails every call.
pub async fn renew(state: &WisaAuthState, refresh_token: &str) -> Result<String, String> {
    match client::post_json(
        "/auth/refresh",
        &json!({ "refreshToken": refresh_token }),
        None,
    )
    .await
    {
        Ok(response) => {
            let renewed = Session::from_auth_response(&response)?;
            persist(&renewed)?;
            let token = renewed.access_token.clone();
            state.store(Some(renewed));
            Ok(token)
        }
        Err(error) => {
            if error.is_unauthorized() {
                forget();
                state.store(None);
                return Err("Phiên đăng nhập đã hết hạn. Đăng nhập lại bằng Google.".to_string());
            }
            Err(error.message)
        }
    }
}

/// Renews the current session in place. Used when a call came back 401: the
/// access token we held is no longer accepted, but the refresh token usually is.
pub async fn refresh(state: &WisaAuthState) -> Result<String, String> {
    let session = ensure_session(state)
        .await?
        .ok_or("Bạn cần đăng nhập WISA trước.")?;
    renew(state, &session.refresh_token).await
}

// ---------------------------------------------------------------------------
// PKCE + loopback
// ---------------------------------------------------------------------------

fn random_b64url(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut buffer);
    URL_SAFE_NO_PAD.encode(buffer)
}

fn sha256_b64url(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        index += 3;
                    }
                    Err(_) => {
                        out.push(bytes[index]);
                        index += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn query_params(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| match pair.split_once('=') {
            Some((key, value)) => (percent_decode(key), percent_decode(value)),
            None => (percent_decode(pair), String::new()),
        })
        .collect()
}

fn param<'a>(params: &'a [(String, String)], name: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

const CALLBACK_PAGE: &str = r#"<!doctype html><html lang="vi"><head><meta charset="utf-8">
<title>WISA Connect</title><style>
:root{color-scheme:light dark}body{font:16px/1.6 system-ui,-apple-system,"Segoe UI",sans-serif;
margin:0;min-height:100vh;display:grid;place-items:center;background:#f7f8fa;color:#111}
.card{max-width:380px;padding:32px;border-radius:16px;background:#fff;box-shadow:0 8px 32px rgb(0 0 0/.08);text-align:center}
h1{font-size:20px;margin:0 0 8px}p{margin:0;color:#5b6472;font-size:14px}
</style></head><body><div class="card"><h1>__TITLE__</h1><p>__BODY__</p></div></body></html>"#;

fn callback_page(title: &str, body: &str) -> String {
    CALLBACK_PAGE
        .replace("__TITLE__", title)
        .replace("__BODY__", body)
}

async fn respond(socket: &mut tokio::net::TcpStream, status: &str, html: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{html}",
        html.len()
    );
    let _ = socket.write_all(response.as_bytes()).await;
    let _ = socket.flush().await;
    let _ = socket.shutdown().await;
}

/// Waits for the one request that carries the authorization code.
///
/// A browser opens speculative connections that never send anything; those are
/// dropped on the idle timeout rather than blocking the wait forever.
async fn await_callback(listener: TcpListener, expected_state: &str) -> Result<String, String> {
    loop {
        let (mut socket, _) = listener
            .accept()
            .await
            .map_err(|error| format!("Không nhận được phản hồi từ Google: {error}"))?;

        let mut buffer = vec![0u8; 8192];
        let read = match tokio::time::timeout(CONNECTION_IDLE_TIMEOUT, socket.read(&mut buffer)).await
        {
            Ok(Ok(count)) => count,
            Ok(Err(_)) | Err(_) => continue,
        };
        if read == 0 {
            continue;
        }

        let request = String::from_utf8_lossy(&buffer[..read]).to_string();
        let Some(target) = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
        else {
            continue;
        };
        let query = target.split_once('?').map(|(_, query)| query).unwrap_or("");
        let params = query_params(query);

        if let Some(error) = param(&params, "error") {
            respond(
                &mut socket,
                "200 OK",
                &callback_page("Chưa đăng nhập được", "Bạn đã huỷ đăng nhập Google. Quay lại WISA Connect và thử lại."),
            )
            .await;
            return Err(format!("Google từ chối đăng nhập ({error})."));
        }

        let Some(code) = param(&params, "code") else {
            // Not the callback (a favicon request, a preconnect): answer and
            // keep waiting.
            respond(&mut socket, "404 Not Found", &callback_page("Không tìm thấy", "Đang chờ đăng nhập.")).await;
            continue;
        };

        if param(&params, "state") != Some(expected_state) {
            respond(
                &mut socket,
                "400 Bad Request",
                &callback_page("Yêu cầu không hợp lệ", "Phiên đăng nhập không khớp. Quay lại WISA Connect và thử lại."),
            )
            .await;
            return Err("Phản hồi đăng nhập không khớp phiên hiện tại.".to_string());
        }

        respond(
            &mut socket,
            "200 OK",
            &callback_page("Đăng nhập thành công", "Bạn có thể đóng tab này và quay lại WISA Connect."),
        )
        .await;
        return Ok(code.to_string());
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Opens the browser, waits for Google, and keeps the session.
#[tauri::command]
pub async fn wisa_login(
    app: tauri::AppHandle,
    state: State<'_, WisaAuthState>,
) -> Result<WisaSessionView, String> {
    let listener = TcpListener::bind(("127.0.0.1", 0u16))
        .await
        .map_err(|error| format!("Không mở được cổng nhận phản hồi đăng nhập: {error}"))?;
    let port = listener
        .local_addr()
        .map_err(|error| format!("Không đọc được cổng nhận phản hồi: {error}"))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}/oauth/callback");

    let verifier = random_b64url(48);
    let challenge = sha256_b64url(&verifier);
    let expected_state = random_b64url(32);

    let authorize = client::post_json(
        "/auth/google/authorize",
        &json!({
            "client": "desktop",
            "redirectUri": redirect_uri,
            "codeChallenge": challenge,
            "state": expected_state,
        }),
        None,
    )
    .await
    .map_err(|error| error.message)?;

    let authorization_url = authorize
        .get("authorizationUrl")
        .and_then(Value::as_str)
        .ok_or("WISA không trả về đường dẫn đăng nhập Google.")?;

    app.opener()
        .open_url(authorization_url, None::<String>)
        .map_err(|error| format!("Không mở được trình duyệt: {error}"))?;

    let code = tokio::time::timeout(CALLBACK_TIMEOUT, await_callback(listener, &expected_state))
        .await
        .map_err(|_| "Hết thời gian chờ đăng nhập Google. Thử lại.".to_string())??;

    let response = client::post_json(
        "/auth/google",
        &json!({
            "client": "desktop",
            "code": code,
            "codeVerifier": verifier,
            "redirectUri": redirect_uri,
        }),
        None,
    )
    .await
    .map_err(|error| error.message)?;

    let session = Session::from_auth_response(&response)?;
    persist(&session)?;
    let view = WisaSessionView::of(&session);
    state.store(Some(session));
    Ok(view)
}

/// The current session, restoring it from the keychain on first call.
#[tauri::command]
pub async fn wisa_session(state: State<'_, WisaAuthState>) -> Result<WisaSessionView, String> {
    match ensure_session(&state).await? {
        Some(session) => Ok(WisaSessionView::of(&session)),
        None => Ok(WisaSessionView::signed_out()),
    }
}

/// Forgets the local session. The account key stays valid on the server: this
/// is "sign out of this app", not "revoke my key".
#[tauri::command]
pub async fn wisa_logout(state: State<'_, WisaAuthState>) -> Result<(), String> {
    forget();
    state.store(None);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_challenge_is_the_sha256_of_the_verifier_in_base64url() {
        // RFC 7636 appendix B.
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        assert_eq!(
            sha256_b64url(verifier),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifier_matches_the_api_validation_pattern() {
        let verifier = random_b64url(48);
        assert!((43..=128).contains(&verifier.len()));
        assert!(verifier
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '~')));
    }

    #[test]
    fn query_params_decode_percent_and_plus() {
        let params = query_params("code=a%2Fb%2Bc&state=x+y&empty");
        assert_eq!(param(&params, "code"), Some("a/b+c"));
        assert_eq!(param(&params, "state"), Some("x y"));
        assert_eq!(param(&params, "empty"), Some(""));
    }

    #[test]
    fn session_view_never_carries_the_tokens() {
        let session = Session {
            user: json!({ "id": "u1", "email": "a@b.c" }),
            access_token: "secret-access".into(),
            refresh_token: "secret-refresh".into(),
        };
        let view = WisaSessionView::of(&session);
        let text = serde_json::to_string(&view).unwrap();
        assert!(!text.contains("secret-access"));
        assert!(!text.contains("secret-refresh"));
        assert!(text.contains("a@b.c"));
    }
}
