use ipc_types::AgentEventDto;
use review_agent::{AgentEvent, AgentEventSink};
use tauri::Emitter;

pub(crate) struct AppAgentEventEmitter(pub tauri::AppHandle);

impl AgentEventSink for AppAgentEventEmitter {
    fn emit(&self, event: AgentEvent) {
        let _ = self.0.emit("agent-event", AgentEventDto::from(event));
    }
}
