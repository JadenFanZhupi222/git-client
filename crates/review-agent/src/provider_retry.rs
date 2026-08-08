use crate::{CancelSignal, ModelProvider, ModelRequest, ModelResponse, ProviderError, RetryPolicy};

#[derive(Debug)]
pub(crate) enum ProviderCallError {
    Cancelled,
    Provider(ProviderError),
}

pub(crate) async fn respond_with_retry(
    model: &dyn ModelProvider,
    request: &ModelRequest,
    cancel: &dyn CancelSignal,
    jitter_key: &str,
    attempts: &mut u32,
) -> Result<ModelResponse, ProviderCallError> {
    let policy = RetryPolicy::default();
    let mut attempt = 1_u8;
    loop {
        if cancel.is_cancelled() {
            return Err(ProviderCallError::Cancelled);
        }
        *attempts = attempts.saturating_add(1);
        let response = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(ProviderCallError::Cancelled),
            response = model.respond(request) => response,
        };
        match response {
            Ok(response) => {
                if cancel.is_cancelled() {
                    return Err(ProviderCallError::Cancelled);
                }
                return Ok(response);
            }
            Err(error) if error.is_transient() && attempt < policy.max_attempts => {
                let delay = policy.delay_after(attempt, jitter_key);
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return Err(ProviderCallError::Cancelled),
                    _ = tokio::time::sleep(delay) => {}
                }
                attempt += 1;
            }
            Err(error) => return Err(ProviderCallError::Provider(error)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModelOutput, ModelUsage, ProviderDescriptor, ResponseFormat, TranscriptItem};
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct NeverCancel;

    impl CancelSignal for NeverCancel {
        fn is_cancelled(&self) -> bool {
            false
        }
    }

    struct SequenceProvider(Mutex<VecDeque<Result<ModelResponse, ProviderError>>>);

    #[async_trait]
    impl ModelProvider for SequenceProvider {
        fn descriptor(&self) -> ProviderDescriptor {
            ProviderDescriptor::unknown()
        }

        async fn respond(&self, _: &ModelRequest) -> Result<ModelResponse, ProviderError> {
            self.0.lock().unwrap().pop_front().unwrap()
        }
    }

    fn request() -> ModelRequest {
        ModelRequest {
            transcript: vec![TranscriptItem::User("fixture".into())],
            tools: Vec::new(),
            response_format: ResponseFormat::JsonObject,
            response_schema: None,
            max_output_tokens: 10,
        }
    }

    #[tokio::test]
    async fn retries_transient_failures_and_returns_the_successful_response() {
        let provider = SequenceProvider(Mutex::new(VecDeque::from([
            Err(ProviderError::Network("offline".into())),
            Ok(ModelResponse {
                output: ModelOutput::FinalText { text: "ok".into() },
                usage: ModelUsage::default(),
            }),
        ])));
        let mut attempts = 0;

        let response = respond_with_retry(
            &provider,
            &request(),
            &NeverCancel,
            "retry-test",
            &mut attempts,
        )
        .await
        .unwrap();

        assert_eq!(attempts, 2);
        assert!(matches!(response.output, ModelOutput::FinalText { .. }));
    }

    #[tokio::test]
    async fn invalid_responses_are_not_retried() {
        let provider = SequenceProvider(Mutex::new(VecDeque::from([
            Err(ProviderError::InvalidResponse("invalid".into())),
            Ok(ModelResponse::final_text("unused", ModelUsage::default())),
        ])));
        let mut attempts = 0;

        let error = respond_with_retry(
            &provider,
            &request(),
            &NeverCancel,
            "no-retry-test",
            &mut attempts,
        )
        .await
        .unwrap_err();

        assert_eq!(attempts, 1);
        assert!(matches!(
            error,
            ProviderCallError::Provider(ProviderError::InvalidResponse(_))
        ));
    }
}
