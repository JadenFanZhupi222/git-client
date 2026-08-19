use crate::{ProviderError, ReviewError};
use reqwest::{Client, Response, StatusCode};
use serde_json::Value;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StatusPolicy {
    Standard,
    DeepSeek,
}

pub(crate) fn build_client(
    connect_timeout: Duration,
    request_timeout: Duration,
) -> Result<Client, ReviewError> {
    Client::builder()
        .connect_timeout(connect_timeout)
        .read_timeout(request_timeout)
        .build()
        .map_err(|_| ReviewError::NetworkError("could not initialize HTTP client".into()))
}

pub(crate) fn map_status(status: StatusCode, policy: StatusPolicy) -> Result<(), ProviderError> {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(ProviderError::AuthFailed),
        StatusCode::PAYMENT_REQUIRED if policy == StatusPolicy::DeepSeek => {
            Err(ProviderError::QuotaExceeded)
        }
        StatusCode::TOO_MANY_REQUESTS => Err(ProviderError::RateLimited),
        status if policy == StatusPolicy::DeepSeek && status.is_client_error() => {
            Err(ProviderError::InvalidRequest)
        }
        status if status.is_server_error() => {
            Err(ProviderError::Network("service request failed".into()))
        }
        status if !status.is_success() => Err(ProviderError::InvalidResponse(
            "service rejected the request".into(),
        )),
        _ => Ok(()),
    }
}

pub(crate) async fn read_json(response: Response) -> Result<Value, ProviderError> {
    let bytes = response
        .bytes()
        .await
        .map_err(|_| ProviderError::Network("response body could not be read".into()))?;
    serde_json::from_slice(&bytes)
        .map_err(|_| ProviderError::Network("service returned an invalid response".into()))
}

pub(crate) fn map_sse_error(error: crate::sse::SseError) -> ProviderError {
    match error {
        crate::sse::SseError::Read(_) => {
            ProviderError::Network("response body could not be read".into())
        }
        _ => ProviderError::InvalidResponse("invalid streaming response".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_status_mapping_preserves_provider_contract() {
        assert_eq!(
            map_status(StatusCode::UNAUTHORIZED, StatusPolicy::Standard),
            Err(ProviderError::AuthFailed)
        );
        assert_eq!(
            map_status(StatusCode::TOO_MANY_REQUESTS, StatusPolicy::Standard),
            Err(ProviderError::RateLimited)
        );
        assert!(matches!(
            map_status(StatusCode::BAD_GATEWAY, StatusPolicy::Standard),
            Err(ProviderError::Network(_))
        ));
        assert!(matches!(
            map_status(StatusCode::BAD_REQUEST, StatusPolicy::Standard),
            Err(ProviderError::InvalidResponse(_))
        ));
        assert_eq!(map_status(StatusCode::OK, StatusPolicy::Standard), Ok(()));
    }

    #[test]
    fn deepseek_status_mapping_keeps_quota_and_invalid_request_semantics() {
        assert_eq!(
            map_status(StatusCode::PAYMENT_REQUIRED, StatusPolicy::DeepSeek),
            Err(ProviderError::QuotaExceeded)
        );
        assert_eq!(
            map_status(StatusCode::BAD_REQUEST, StatusPolicy::DeepSeek),
            Err(ProviderError::InvalidRequest)
        );
        assert!(matches!(
            map_status(StatusCode::BAD_GATEWAY, StatusPolicy::DeepSeek),
            Err(ProviderError::Network(_))
        ));
    }
}
