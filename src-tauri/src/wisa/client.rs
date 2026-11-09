//! Thin JSON client for the WISA account API and the AI gateway.
//!
//! The base URLs are overridable through the environment so a dev build can
//! talk to `apps/api` on `127.0.0.1:4000` without a rebuild; a packaged build
//! always uses the defaults.

use once_cell::sync::Lazy;
use serde_json::Value;
use std::time::Duration;

/// Where account calls go: login, the setup manifest, the account key.
pub const DEFAULT_ACCOUNT_API_BASE: &str = "https://gateway.wisa.vn/studio/v1";
/// Where inference goes, and where the model list comes from.
pub const DEFAULT_GATEWAY_BASE: &str = "https://ai.wisa.vn";

static HTTP: Lazy<reqwest::Client> = Lazy::new(|| {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(concat!("WISA Connect/", env!("CARGO_PKG_VERSION")))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
});

pub fn account_api_base() -> String {
    override_or("WISA_ACCOUNT_API_BASE", DEFAULT_ACCOUNT_API_BASE)
}

pub fn gateway_base() -> String {
    override_or("WISA_GATEWAY_BASE", DEFAULT_GATEWAY_BASE)
}

fn override_or(key: &str, fallback: &str) -> String {
    match std::env::var(key) {
        Ok(value) if !value.trim().is_empty() => value.trim().trim_end_matches('/').to_string(),
        _ => fallback.to_string(),
    }
}

/// A failed call: the HTTP status (0 when the request never reached the API)
/// and the sentence to show the user.
#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: u16,
    pub message: String,
}

impl ApiError {
    pub fn is_not_found(&self) -> bool {
        self.status == 404
    }

    pub fn is_unauthorized(&self) -> bool {
        self.status == 401 || self.status == 403
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

pub async fn post_json(path: &str, body: &Value, token: Option<&str>) -> Result<Value, ApiError> {
    let url = format!("{}{}", account_api_base(), path);
    let mut request = HTTP.post(url).json(body);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    send(request).await
}

pub async fn get_json(path: &str, token: Option<&str>) -> Result<Value, ApiError> {
    get_absolute(&format!("{}{}", account_api_base(), path), token).await
}

pub async fn get_absolute(url: &str, token: Option<&str>) -> Result<Value, ApiError> {
    let mut request = HTTP.get(url);
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    send(request).await
}

async fn send(request: reqwest::RequestBuilder) -> Result<Value, ApiError> {
    let response = request.send().await.map_err(|error| ApiError {
        status: 0,
        message: format!("Không kết nối được tới WISA: {error}"),
    })?;
    let status = response.status().as_u16();
    let body = response.text().await.unwrap_or_default();
    if !(200..300).contains(&status) {
        return Err(describe(status, &body));
    }
    if body.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&body).map_err(|error| ApiError {
        status,
        message: format!("Phản hồi từ WISA không đọc được: {error}"),
    })
}

/// Nest answers `{ message: string | string[] }`; anything else falls back to
/// the status so the user never sees an empty error box.
fn describe(status: u16, body: &str) -> ApiError {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| match value.get("message") {
            Some(Value::String(text)) => Some(text.clone()),
            Some(Value::Array(items)) => Some(
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(" "),
            ),
            _ => None,
        })
        .filter(|text| !text.trim().is_empty())
        .unwrap_or_else(|| format!("WISA trả về lỗi {status}."));
    ApiError { status, message }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_prefers_the_api_sentence() {
        let error = describe(409, r#"{"message":"Key đã tồn tại"}"#);
        assert_eq!(error.status, 409);
        assert_eq!(error.message, "Key đã tồn tại");
    }

    #[test]
    fn describe_joins_validation_messages() {
        let error = describe(400, r#"{"message":["a","b"]}"#);
        assert_eq!(error.message, "a b");
    }

    #[test]
    fn describe_falls_back_to_the_status() {
        assert_eq!(describe(500, "<html>").message, "WISA trả về lỗi 500.");
    }
}
