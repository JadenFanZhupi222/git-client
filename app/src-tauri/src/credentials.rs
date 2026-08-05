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
        _ => ("CREDENTIAL_TEST_FAILED", true),
    };
    IpcError {
        code: code.into(),
        message: "Credential validation failed".into(),
        recoverable,
    }
}

async fn test_with_client(
    client: &Client,
    kind: CredentialKindDto,
    secret: &str,
) -> Result<(), IpcError> {
    let request = match kind {
        CredentialKindDto::Github => client
            .get("https://api.github.com/user")
            .bearer_auth(secret)
            .header("User-Agent", "git-client"),
        CredentialKindDto::Gitlab => client
            .get("https://gitlab.com/api/v4/user")
            .header("PRIVATE-TOKEN", secret),
        CredentialKindDto::Deepseek => client
            .get("https://api.deepseek.com/models")
            .bearer_auth(secret),
    };
    let response = request.send().await.map_err(|_| IpcError {
        code: "NETWORK_ERROR".into(),
        message: "Credential validation request failed".into(),
        recoverable: true,
    })?;
    if response.status().is_success() {
        Ok(())
    } else {
        Err(http_status_error(response.status()))
    }
}

#[tauri::command]
pub(crate) async fn test_credential(kind: CredentialKindDto) -> Result<(), IpcError> {
    let secret = tokio::task::spawn_blocking(move || read_credential(kind))
        .await
        .map_err(crate::join_panic)??;
    test_with_client(&Client::new(), kind, &secret).await
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
