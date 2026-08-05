use ipc_types::{CredentialKindDto, IpcError};
use reqwest::{Client, StatusCode};

const SERVICE: &str = "com.gitclient.desktop";

pub(crate) fn credential_user(kind: CredentialKindDto) -> &'static str {
    match kind {
        CredentialKindDto::Github => "github-token",
        CredentialKindDto::Gitlab => "gitlab-token",
        CredentialKindDto::Deepseek => "deepseek-token",
    }
}

pub(crate) fn normalize_secret(secret: String) -> Result<String, IpcError> {
    let secret = secret.trim().to_owned();
    if secret.is_empty() {
        Err(IpcError {
            code: "CREDENTIAL_EMPTY".into(),
            message: "Credential cannot be empty".into(),
            recoverable: false,
        })
    } else {
        Ok(secret)
    }
}

fn keyring_error(error: keyring::Error) -> IpcError {
    IpcError {
        code: "KEYRING".into(),
        message: error.to_string(),
        recoverable: true,
    }
}

fn entry(kind: CredentialKindDto) -> Result<keyring::Entry, IpcError> {
    keyring::Entry::new(SERVICE, credential_user(kind)).map_err(keyring_error)
}

pub(crate) fn read_credential(kind: CredentialKindDto) -> Result<String, IpcError> {
    match entry(kind)?.get_password() {
        Ok(secret) => Ok(secret),
        Err(keyring::Error::NoEntry) => Err(IpcError {
            code: "CREDENTIAL_MISSING".into(),
            message: "Credential is not configured".into(),
            recoverable: true,
        }),
        Err(error) => Err(keyring_error(error)),
    }
}

#[tauri::command]
pub(crate) async fn credential_status(kind: CredentialKindDto) -> Result<bool, IpcError> {
    tokio::task::spawn_blocking(move || match entry(kind)?.get_password() {
        Ok(_) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(keyring_error(e)),
    })
    .await
    .map_err(crate::join_panic)?
}

#[tauri::command]
pub(crate) async fn save_credential(
    kind: CredentialKindDto,
    secret: String,
) -> Result<(), IpcError> {
    let secret = normalize_secret(secret)?;
    tokio::task::spawn_blocking(move || entry(kind)?.set_password(&secret).map_err(keyring_error))
        .await
        .map_err(crate::join_panic)?
}

#[tauri::command]
pub(crate) async fn clear_credential(kind: CredentialKindDto) -> Result<(), IpcError> {
    tokio::task::spawn_blocking(move || match entry(kind)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(keyring_error(e)),
    })
    .await
    .map_err(crate::join_panic)?
}

pub(crate) fn http_status_error(status: StatusCode) -> IpcError {
    let (code, recoverable) = match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => ("AUTH_FAILED", false),
        StatusCode::TOO_MANY_REQUESTS => ("RATE_LIMITED", true),
        status if status.is_server_error() => ("NETWORK_ERROR", true),
        _ => ("CREDENTIAL_TEST_FAILED", true),
    };
    IpcError {
        code: code.into(),
        message: "Credential validation failed".into(),
        recoverable,
    }
}

struct EndpointConfig {
    deepseek: String,
    github: String,
    gitlab: String,
}

impl EndpointConfig {
    fn production() -> Self {
        Self {
            deepseek: "https://api.deepseek.com/models".into(),
            github: "https://api.github.com/user".into(),
            gitlab: "https://gitlab.com/api/v4/user".into(),
        }
    }

    #[cfg(test)]
    fn for_test(base: &str) -> Self {
        Self {
            deepseek: format!("{base}/models"),
            github: format!("{base}/user"),
            gitlab: format!("{base}/api/v4/user"),
        }
    }
}

async fn validate_credential(
    client: &Client,
    endpoints: &EndpointConfig,
    kind: CredentialKindDto,
    secret: &str,
) -> Result<(), IpcError> {
    let request = match kind {
        CredentialKindDto::Github => client
            .get(&endpoints.github)
            .bearer_auth(secret)
            .header("User-Agent", "git-client"),
        CredentialKindDto::Gitlab => client
            .get(&endpoints.gitlab)
            .header("PRIVATE-TOKEN", secret),
        CredentialKindDto::Deepseek => client.get(&endpoints.deepseek).bearer_auth(secret),
    };
    let response = request.send().await.map_err(|_| IpcError {
        code: "NETWORK_ERROR".into(),
        message: "Credential validation request failed".into(),
        recoverable: true,
    })?;
    if response.status().is_success() {
        Ok(())
    } else if kind == CredentialKindDto::Github
        && response.status() == StatusCode::FORBIDDEN
        && response
            .headers()
            .get("x-ratelimit-remaining")
            .is_some_and(|value| value == "0")
    {
        Err(http_status_error(StatusCode::TOO_MANY_REQUESTS))
    } else {
        Err(http_status_error(response.status()))
    }
}

#[tauri::command]
pub(crate) async fn test_credential(kind: CredentialKindDto) -> Result<(), IpcError> {
    let secret = tokio::task::spawn_blocking(move || read_credential(kind))
        .await
        .map_err(crate::join_panic)??;
    validate_credential(&Client::new(), &EndpointConfig::production(), kind, &secret).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn credential_kind_preserves_legacy_keyring_names() {
        assert_eq!(credential_user(CredentialKindDto::Github), "github-token");
        assert_eq!(credential_user(CredentialKindDto::Gitlab), "gitlab-token");
        assert_eq!(
            credential_user(CredentialKindDto::Deepseek),
            "deepseek-token"
        );
    }

    #[test]
    fn whitespace_secret_has_stable_error_without_echoing_value() {
        let error = normalize_secret("  \n".into()).unwrap_err();
        assert_eq!(error.code, "CREDENTIAL_EMPTY");
        assert!(!error.message.contains('\n'));
    }

    #[test]
    fn provider_error_mapping_is_stable() {
        assert_eq!(
            http_status_error(reqwest::StatusCode::UNAUTHORIZED).code,
            "AUTH_FAILED"
        );
        assert_eq!(
            http_status_error(reqwest::StatusCode::TOO_MANY_REQUESTS).code,
            "RATE_LIMITED"
        );
    }

    #[tokio::test]
    async fn validates_each_provider_at_fixed_path_with_expected_auth_header() {
        for (kind, expected_path, header_name, header_value) in [
            (
                CredentialKindDto::Deepseek,
                "/models",
                "authorization",
                "Bearer fixture-secret",
            ),
            (
                CredentialKindDto::Github,
                "/user",
                "authorization",
                "Bearer fixture-secret",
            ),
            (
                CredentialKindDto::Gitlab,
                "/api/v4/user",
                "private-token",
                "fixture-secret",
            ),
        ] {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path(expected_path))
                .and(header(header_name, header_value))
                .respond_with(ResponseTemplate::new(200))
                .expect(1)
                .mount(&server)
                .await;
            validate_credential(
                &Client::new(),
                &EndpointConfig::for_test(&server.uri()),
                kind,
                "fixture-secret",
            )
            .await
            .unwrap();
        }
    }

    #[tokio::test]
    async fn maps_auth_rate_limit_server_and_timeout_without_leaking_secret() {
        for (status, headers, expected) in [
            (401, vec![], "AUTH_FAILED"),
            (403, vec![], "AUTH_FAILED"),
            (403, vec![("x-ratelimit-remaining", "0")], "RATE_LIMITED"),
            (429, vec![], "RATE_LIMITED"),
            (500, vec![], "NETWORK_ERROR"),
        ] {
            let server = MockServer::start().await;
            let mut response = ResponseTemplate::new(status);
            for (name, value) in headers {
                response = response.insert_header(name, value);
            }
            Mock::given(method("GET"))
                .and(path("/user"))
                .respond_with(response)
                .mount(&server)
                .await;
            let error = validate_credential(
                &Client::new(),
                &EndpointConfig::for_test(&server.uri()),
                CredentialKindDto::Github,
                "fixture-secret",
            )
            .await
            .unwrap_err();
            assert_eq!(error.code, expected);
            assert!(
                !serde_json::to_string(&error)
                    .unwrap()
                    .contains("fixture-secret")
            );
        }

        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(100)))
            .mount(&server)
            .await;
        let client = Client::builder()
            .timeout(Duration::from_millis(20))
            .build()
            .unwrap();
        let error = validate_credential(
            &client,
            &EndpointConfig::for_test(&server.uri()),
            CredentialKindDto::Deepseek,
            "fixture-secret",
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "NETWORK_ERROR");
        assert!(
            !serde_json::to_string(&error)
                .unwrap()
                .contains("fixture-secret")
        );

        let client = Client::builder()
            .timeout(Duration::from_millis(100))
            .build()
            .unwrap();
        let error = validate_credential(
            &client,
            &EndpointConfig::for_test("http://127.0.0.1:9"),
            CredentialKindDto::Gitlab,
            "fixture-secret",
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "NETWORK_ERROR");
        assert!(
            !serde_json::to_string(&error)
                .unwrap()
                .contains("fixture-secret")
        );
    }
}
