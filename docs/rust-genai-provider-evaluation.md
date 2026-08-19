# rust-genai provider adapter evaluation

Evaluated: 2026-08-19  
Candidate: `genai` 0.6.5 (`rust-genai`)

The experiment used the existing provider contract as the acceptance boundary. The dependency and
feature-gated adapter were removed after the all-or-nothing gate failed; production dependencies are
therefore unchanged.

| Required capability | Result | Notes |
| --- | --- | --- |
| Provider-neutral transcript/tool/result mapping | Passable | The candidate has neutral messages, tools, tool results, and tool-call chunks. |
| Structured output | Passable | JSON Schema is supported for OpenAI and Anthropic, with provider normalization. |
| Streaming deltas | Fail | The normalized stream does not preserve the exact provider event transitions asserted by the current OpenAI Responses and Anthropic contract tests. Keeping our state machines around it would duplicate the adapter rather than replace it. |
| Cache usage | Passable | Normalized cached and cache-creation token fields exist. |
| Error classification | Fail | The application contract distinguishes DeepSeek payment/quota and invalid-request responses and intentionally classifies malformed response bodies as network failures. The candidate error surface does not preserve this wire-level mapping without a second HTTP adapter. |
| Custom base URL | Passable | Explicit service targets/endpoints are supported. |
| DeepSeek DSML recovery | Fail | Complete-DSML recovery and streamed DSML suppression are application-specific and operate before the neutral response exists. The candidate's OpenAI-compatible DeepSeek adapter does not provide this contract. |

Because any failed requirement prohibits a partial migration, no production provider was moved.
The retained change is an internal `provider_http` module for client construction, HTTP status
classification, JSON body reads, and SSE error classification. Request encoding, streaming state,
termination handling, usage parsing, and DSML recovery remain provider-specific.
