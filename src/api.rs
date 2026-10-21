use reqwest::{Client, Method, StatusCode};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use url::Url;

use crate::{
    error::{CliError, Result},
    model::{ReportResponse, ResourcesResponse, SessionResponse, StartTimerRequest, TimerResponse},
};

#[derive(Debug, Clone)]
pub struct ApiClient {
    http: Client,
    base_url: Url,
    token: Option<String>,
}

impl ApiClient {
    pub fn new(base_url: &str, token: Option<String>) -> Result<Self> {
        let base_url = validate_base_url(base_url)?;
        let http = Client::builder()
            .user_agent(concat!("ttrackii-cli/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            base_url,
            token,
        })
    }

    #[must_use]
    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    pub async fn session(&self) -> Result<SessionResponse> {
        self.request(Method::GET, "/api/v1/session", Option::<&()>::None)
            .await
    }

    pub async fn resources(&self, workspace_id: &str) -> Result<ResourcesResponse> {
        let path = format!(
            "/api/v1/resources?workspaceId={}",
            url::form_urlencoded::byte_serialize(workspace_id.as_bytes()).collect::<String>()
        );
        self.request(Method::GET, &path, Option::<&()>::None).await
    }

    pub async fn timer(&self) -> Result<TimerResponse> {
        self.request(Method::GET, "/api/v1/timer", Option::<&()>::None)
            .await
    }

    pub async fn start_timer(&self, request: &StartTimerRequest) -> Result<TimerResponse> {
        self.request(Method::POST, "/api/v1/timer/start", Some(request))
            .await
    }

    pub async fn stop_timer(&self) -> Result<TimerResponse> {
        self.request(
            Method::POST,
            "/api/v1/timer/stop",
            Some(&serde_json::json!({})),
        )
        .await
    }

    pub async fn report_page(
        &self,
        workspace_id: &str,
        start_date: &str,
        end_date: &str,
        scope: &str,
        cursor: Option<&str>,
        limit: u16,
    ) -> Result<ReportResponse> {
        let mut url = self.endpoint("/api/v1/report")?;
        {
            let mut query = url.query_pairs_mut();
            query
                .append_pair("workspaceId", workspace_id)
                .append_pair("startDate", start_date)
                .append_pair("endDate", end_date)
                .append_pair("scope", scope)
                .append_pair("limit", &limit.to_string());
            if let Some(cursor) = cursor {
                query.append_pair("cursor", cursor);
            }
        }
        self.request_url(Method::GET, url, Option::<&()>::None)
            .await
    }

    /// Revoke a CLI device session. No-op for API keys (`tt_live_…`).
    pub async fn revoke(&self) -> Result<()> {
        let Some(token) = self.token.as_deref() else {
            return Ok(());
        };
        if is_api_key_token(token) {
            return Ok(());
        }
        // Prefer revoke-session so the bearer token itself is deleted.
        // Better Auth's sign-out only clears cookies unless the bearer plugin
        // rehydrates them — revoke-session is explicit for CLI sessions.
        let response: Value = self
            .request(
                Method::POST,
                "/api/auth/revoke-session",
                Some(&serde_json::json!({ "token": token })),
            )
            .await?;
        drop(response);
        Ok(())
    }

    pub(crate) fn endpoint(&self, path: &str) -> Result<Url> {
        self.base_url
            .join(path)
            .map_err(|error| CliError::InvalidInput(format!("invalid API endpoint: {error}")))
    }

    pub(crate) async fn request<B, T>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let url = self.endpoint(path)?;
        self.request_url(method, url, body).await
    }

    pub(crate) async fn request_url<B, T>(
        &self,
        method: Method,
        url: Url,
        body: Option<&B>,
    ) -> Result<T>
    where
        B: Serialize + ?Sized,
        T: DeserializeOwned,
    {
        let mut request = self.http.request(method, url);
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await?;
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED {
            return Err(CliError::AuthRequired);
        }
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(CliError::Api {
                status,
                message: api_error_message(&body),
            });
        }
        response
            .json()
            .await
            .map_err(|error| CliError::Response(error.to_string()))
    }
}

pub fn validate_base_url(value: &str) -> Result<Url> {
    let mut url = Url::parse(value)
        .map_err(|error| CliError::InvalidInput(format!("invalid API URL: {error}")))?;
    let localhost = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if url.scheme() != "https" && !(url.scheme() == "http" && localhost) {
        return Err(CliError::InvalidInput(
            "the API URL must use HTTPS (HTTP is allowed only for localhost)".into(),
        ));
    }
    url.set_query(None);
    url.set_fragment(None);
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    Ok(url)
}

/// Official API keys from Settings → API & CLI (not device sessions).
#[must_use]
pub fn is_api_key_token(token: &str) -> bool {
    token.starts_with("tt_live_") && token.len() > "tt_live_".len() + 16
}

fn api_error_message(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .get("message")
                .or_else(|| value.get("error_description"))
                .or_else(|| value.get("error"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .filter(|message| !message.is_empty())
        .unwrap_or_else(|| "request rejected by server".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_insecure_remote_origins() {
        assert!(validate_base_url("http://example.com").is_err());
        assert!(validate_base_url("https://example.com").is_ok());
        assert!(validate_base_url("http://localhost:3000").is_ok());
        assert!(validate_base_url("http://127.0.0.1:3000").is_ok());
    }

    #[test]
    fn extracts_safe_api_messages() {
        assert_eq!(
            api_error_message(r#"{"message":"not allowed"}"#),
            "not allowed"
        );
        assert_eq!(api_error_message("not json"), "request rejected by server");
    }

    #[test]
    fn detects_official_api_keys() {
        assert!(is_api_key_token("tt_live_abcdefghijklmnopqrstuvwx"));
        assert!(!is_api_key_token("session-token"));
        assert!(!is_api_key_token("tt_live_short"));
    }
}
