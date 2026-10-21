use std::{env, sync::Arc, time::Duration};

use keyring::{Entry, Error as KeyringError};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::time::{Instant, sleep};

use crate::{
    api::validate_base_url,
    error::{CliError, Result},
};

pub const KEYRING_SERVICE: &str = "com.ttrackii.cli";
const KEYRING_USER: &str = "session";
const CLIENT_ID: &str = "ttrackii-cli";
const DEVICE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenSource {
    Environment,
    Keyring,
}

#[derive(Debug, Clone)]
pub struct Credential {
    pub token: String,
    pub source: TokenSource,
}

pub trait CredentialStore: Send + Sync {
    fn get(&self) -> Result<Option<String>>;
    fn set(&self, token: &str) -> Result<()>;
    fn delete(&self) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct KeyringStore;

impl KeyringStore {
    fn entry() -> Result<Entry> {
        Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|error| CliError::SecureStorage(error.to_string()))
    }
}

impl CredentialStore for KeyringStore {
    fn get(&self) -> Result<Option<String>> {
        match Self::entry()?.get_password() {
            Ok(token) => Ok(Some(token)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(error) => Err(CliError::SecureStorage(error.to_string())),
        }
    }

    fn set(&self, token: &str) -> Result<()> {
        Self::entry()?
            .set_password(token)
            .map_err(|error| CliError::SecureStorage(error.to_string()))
    }

    fn delete(&self) -> Result<()> {
        match Self::entry()?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(error) => Err(CliError::SecureStorage(error.to_string())),
        }
    }
}

#[derive(Clone)]
pub struct TokenManager {
    store: Arc<dyn CredentialStore>,
}

impl std::fmt::Debug for TokenManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TokenManager")
            .finish_non_exhaustive()
    }
}

impl Default for TokenManager {
    fn default() -> Self {
        Self::new(Arc::new(KeyringStore))
    }
}

impl TokenManager {
    #[must_use]
    pub const fn new(store: Arc<dyn CredentialStore>) -> Self {
        Self { store }
    }

    pub fn load(&self) -> Result<Option<Credential>> {
        if let Ok(token) = env::var("TTRACKII_TOKEN") {
            if !token.trim().is_empty() {
                return Ok(Some(Credential {
                    token,
                    source: TokenSource::Environment,
                }));
            }
        }
        Ok(self.store.get()?.map(|token| Credential {
            token,
            source: TokenSource::Keyring,
        }))
    }

    pub fn save(&self, token: &str) -> Result<()> {
        self.store.set(token)
    }

    pub fn delete_stored(&self) -> Result<()> {
        self.store.delete()
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct OAuthError {
    error: String,
    #[serde(default)]
    error_description: Option<String>,
}

#[derive(Debug, Serialize)]
struct DeviceCodeRequest<'a> {
    client_id: &'a str,
}

#[derive(Debug, Serialize)]
struct TokenRequest<'a> {
    grant_type: &'a str,
    device_code: &'a str,
    client_id: &'a str,
}

#[derive(Debug, Clone)]
pub struct DeviceAuth {
    http: Client,
    base_url: url::Url,
}

impl DeviceAuth {
    pub fn new(base_url: &str) -> Result<Self> {
        Ok(Self {
            http: Client::builder()
                .user_agent(concat!("ttrackii-cli/", env!("CARGO_PKG_VERSION")))
                .build()?,
            base_url: validate_base_url(base_url)?,
        })
    }

    pub async fn request_code(&self) -> Result<DeviceCode> {
        let url = self
            .base_url
            .join("/api/auth/device/code")
            .map_err(|error| CliError::Authentication(error.to_string()))?;
        let response = self
            .http
            .post(url)
            .json(&DeviceCodeRequest {
                client_id: CLIENT_ID,
            })
            .send()
            .await?;
        decode_auth_response(response).await
    }

    pub async fn poll_token(&self, code: &DeviceCode) -> Result<String> {
        let url = self
            .base_url
            .join("/api/auth/device/token")
            .map_err(|error| CliError::Authentication(error.to_string()))?;
        let deadline = Instant::now() + Duration::from_secs(code.expires_in);
        let mut interval = Duration::from_secs(code.interval.max(1));

        loop {
            if Instant::now() >= deadline {
                return Err(CliError::Authentication("device code expired".into()));
            }
            tokio::select! {
                () = sleep(interval) => {}
                signal = tokio::signal::ctrl_c() => {
                    signal.map_err(CliError::Io)?;
                    return Err(CliError::Authentication("login cancelled".into()));
                }
            }
            let response = self
                .http
                .post(url.clone())
                .json(&TokenRequest {
                    grant_type: DEVICE_GRANT,
                    device_code: &code.device_code,
                    client_id: CLIENT_ID,
                })
                .send()
                .await?;

            if response.status().is_success() {
                let token = response
                    .json::<TokenResponse>()
                    .await
                    .map_err(|error| CliError::Response(error.to_string()))?;
                return Ok(token.access_token);
            }

            let status = response.status();
            let error = response.json::<OAuthError>().await.unwrap_or(OAuthError {
                error: "server_error".into(),
                error_description: None,
            });
            match error.error.as_str() {
                "authorization_pending" => {}
                "slow_down" => interval += Duration::from_secs(5),
                "access_denied" => {
                    return Err(CliError::Authentication("authorization denied".into()));
                }
                "expired_token" => {
                    return Err(CliError::Authentication("device code expired".into()));
                }
                _ => {
                    return Err(CliError::Authentication(
                        error
                            .error_description
                            .unwrap_or_else(|| format!("token request failed with HTTP {status}")),
                    ));
                }
            }
        }
    }
}

async fn decode_auth_response<T: serde::de::DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T> {
    let status = response.status();
    if !status.is_success() {
        let error = response.json::<OAuthError>().await.unwrap_or(OAuthError {
            error: "server_error".into(),
            error_description: None,
        });
        return Err(CliError::Authentication(
            error
                .error_description
                .unwrap_or_else(|| format!("{} (HTTP {status})", error.error)),
        ));
    }
    response
        .json()
        .await
        .map_err(|error| CliError::Response(error.to_string()))
}

#[must_use]
pub fn verification_url(code: &DeviceCode) -> &str {
    code.verification_uri_complete
        .as_deref()
        .unwrap_or(&code.verification_uri)
}

pub fn open_browser(code: &DeviceCode) -> Result<()> {
    open::that_detached(verification_url(code))
        .map_err(|error| CliError::Authentication(format!("could not open browser: {error}")))
}

const fn default_interval() -> u64 {
    5
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    #[derive(Default)]
    struct MemoryStore(Mutex<Option<String>>);

    impl CredentialStore for MemoryStore {
        fn get(&self) -> Result<Option<String>> {
            Ok(self.0.lock().unwrap().clone())
        }

        fn set(&self, token: &str) -> Result<()> {
            *self.0.lock().unwrap() = Some(token.into());
            Ok(())
        }

        fn delete(&self) -> Result<()> {
            *self.0.lock().unwrap() = None;
            Ok(())
        }
    }

    #[test]
    fn stored_token_round_trips() {
        let manager = TokenManager::new(Arc::new(MemoryStore::default()));
        manager.save("secret-value").unwrap();
        let credential = manager.load().unwrap().unwrap();
        assert_eq!(credential.token, "secret-value");
        assert_eq!(credential.source, TokenSource::Keyring);
        manager.delete_stored().unwrap();
        assert!(manager.load().unwrap().is_none());
    }

    #[test]
    fn prefers_complete_verification_url() {
        let code = DeviceCode {
            device_code: "device".into(),
            user_code: "ABCD-1234".into(),
            verification_uri: "https://example.com/device".into(),
            verification_uri_complete: Some(
                "https://example.com/device?user_code=ABCD-1234".into(),
            ),
            expires_in: 600,
            interval: 5,
        };
        assert!(verification_url(&code).contains("user_code="));
    }
}
