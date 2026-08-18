use crate::{
    AgentEventKind, AgentEventPublisher, CancelSignal, ModelProvider, ModelRequest, ModelResponse,
    NoopAgentEventSink, ProviderError, RetryPolicy,
};

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
    let event_context = AgentEventPublisher::new(jitter_key, &NoopAgentEventSink);
    respond_with_retry_and_events(model, request, cancel, attempts, &event_context).await
}

pub(crate) async fn respond_with_retry_and_events(
    model: &dyn ModelProvider,
    request: &ModelRequest,
    cancel: &dyn CancelSignal,
    attempts: &mut u32,
    event_context: &AgentEventPublisher<'_>,
) -> Result<ModelResponse, ProviderCallError> {
    let policy = RetryPolicy::default();
    let mut attempt = 1_u8;
    loop {
        if cancel.is_cancelled() {
            return Err(ProviderCallError::Cancelled);
        }
        *attempts = attempts.saturating_add(1);
        let events = event_context.next_attempt();
        let descriptor = model.descriptor();
        events.emit(AgentEventKind::ModelAttemptStarted {
            provider_id: descriptor.provider_id,
            model_id: descriptor.model_id,
        });
        let response = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(ProviderCallError::Cancelled),
            response = model.respond_stream(request, &events) => response,
        };
        match response {
            Ok(response) => {
                if cancel.is_cancelled() {
                    return Err(ProviderCallError::Cancelled);
                }
                return Ok(response);
            }
            Err(error) if error.is_transient() && attempt < policy.max_attempts => {
                events.emit(AgentEventKind::ModelAttemptFailed {
                    error: (&error).into(),
                    will_retry: true,
                });
                let delay = policy.delay_after(attempt, event_context.run_id());
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => return Err(ProviderCallError::Cancelled),
                    _ = tokio::time::sleep(delay) => {}
                }
                attempt += 1;
            }
            Err(error) => {
                events.emit(AgentEventKind::ModelAttemptFailed {
                    error: (&error).into(),
                    will_retry: false,
                });
                return Err(ProviderCallError::Provider(error));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AgentEventSink, ModelOutput, ModelUsage, ProviderDescriptor, ResponseFormat, TranscriptItem,
    };
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

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

    #[derive(Default)]
    struct RecordingSink(Mutex<Vec<crate::AgentEvent>>);

    impl AgentEventSink for RecordingSink {
        fn emit(&self, event: crate::AgentEvent) {
            self.0.lock().unwrap().push(event);
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

    #[tokio::test]
    async fn retry_events_have_distinct_attempts_and_monotonic_sequences() {
        let provider = SequenceProvider(Mutex::new(VecDeque::from([
            Err(ProviderError::RateLimited),
            Ok(ModelResponse::final_text("ok", ModelUsage::default())),
        ])));
        let sink = RecordingSink::default();
        let event_context = AgentEventPublisher::new("run-stream", &sink);
        let mut attempts = 0;

        respond_with_retry_and_events(
            &provider,
            &request(),
            &NeverCancel,
            &mut attempts,
            &event_context,
        )
        .await
        .unwrap();

        let events = sink.0.lock().unwrap();
        assert_eq!(
            events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            (1..8).collect::<Vec<_>>()
        );
        assert_eq!(
            events
                .iter()
                .filter_map(|event| match event.kind {
                    AgentEventKind::ModelAttemptStarted { .. } => Some(event.attempt_id),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(matches!(
            events[2].kind,
            AgentEventKind::ModelAttemptFailed {
                will_retry: true,
                ..
            }
        ));
    }

    struct SlowProvider;

    #[async_trait]
    impl ModelProvider for SlowProvider {
        fn descriptor(&self) -> ProviderDescriptor {
            ProviderDescriptor::unknown()
        }

        async fn respond(&self, _: &ModelRequest) -> Result<ModelResponse, ProviderError> {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            Ok(ModelResponse::final_text("late", ModelUsage::default()))
        }
    }

    struct ToggleCancel(std::sync::atomic::AtomicBool);

    impl CancelSignal for ToggleCancel {
        fn is_cancelled(&self) -> bool {
            self.0.load(std::sync::atomic::Ordering::Relaxed)
        }
    }

    #[tokio::test]
    async fn cancellation_interrupts_an_in_flight_streaming_call() {
        let cancel = Arc::new(ToggleCancel(std::sync::atomic::AtomicBool::new(false)));
        let trigger = Arc::clone(&cancel);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            trigger.0.store(true, std::sync::atomic::Ordering::Relaxed);
        });
        let mut attempts = 0;
        let result = respond_with_retry(
            &SlowProvider,
            &request(),
            cancel.as_ref(),
            "cancel-test",
            &mut attempts,
        )
        .await;
        assert!(matches!(result, Err(ProviderCallError::Cancelled)));
        assert_eq!(attempts, 1);
    }
}
