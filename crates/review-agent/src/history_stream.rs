use crate::{AgentEventAugmenter, AgentEventKind};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

pub const HISTORY_ARTIFACT_TYPE: &str = "history_investigation";

#[derive(Default)]
pub struct HistoryEventAugmenter {
    attempts: Mutex<HashMap<u32, HistoryArtifactStream>>,
}

impl AgentEventAugmenter for HistoryEventAugmenter {
    fn additional_events(&self, attempt_id: u32, kind: &AgentEventKind) -> Vec<AgentEventKind> {
        let mut attempts = self
            .attempts
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if matches!(kind, AgentEventKind::ModelAttemptStarted { .. }) {
            attempts.insert(attempt_id, HistoryArtifactStream::default());
            return Vec::new();
        }
        let AgentEventKind::OutputTextDelta { delta } = kind else {
            return Vec::new();
        };
        attempts.entry(attempt_id).or_default().feed(delta)
    }
}

#[derive(Default)]
struct HistoryArtifactStream {
    frames: Vec<Frame>,
    string: Option<JsonStringState>,
    primitive: bool,
    root_started: bool,
    root_complete: bool,
    failed: bool,
}

impl HistoryArtifactStream {
    fn feed(&mut self, chunk: &str) -> Vec<AgentEventKind> {
        let mut events = Vec::new();
        for character in chunk.chars() {
            if self.failed || self.root_complete {
                continue;
            }
            if let Some(mut string) = self.string.take() {
                match string.consume(character) {
                    StringAction::Continue(decoded) => {
                        if let Some(decoded) = decoded {
                            string.push_decoded(decoded, &mut events);
                        }
                        self.string = Some(string);
                    }
                    StringAction::Complete => self.complete_string(string),
                    StringAction::Failed => self.failed = true,
                }
                continue;
            }
            if self.primitive {
                if matches!(character, ',' | '}' | ']') {
                    self.primitive = false;
                    self.handle_structural(character, &mut events);
                }
                continue;
            }
            self.handle_structural(character, &mut events);
        }
        events
    }

    fn handle_structural(&mut self, character: char, events: &mut Vec<AgentEventKind>) {
        if !self.root_started {
            if character == '{' {
                self.root_started = true;
                self.frames.push(Frame::object(ObjectRole::Root));
            }
            return;
        }
        if character.is_ascii_whitespace() {
            return;
        }
        match character {
            '"' => self.start_string(events),
            ':' => self.accept_colon(),
            ',' => self.accept_comma(),
            '{' => self.start_object(),
            '}' => self.close_object(),
            '[' => self.start_array(),
            ']' => self.close_array(),
            _ if self.mark_scalar_value_complete() => self.primitive = true,
            _ => self.failed = true,
        }
    }

    fn start_string(&mut self, events: &mut Vec<AgentEventKind>) {
        let purpose = match self.frames.last() {
            Some(Frame::Object {
                state: ObjectState::KeyOrEnd,
                ..
            }) => StringPurpose::Key(String::new()),
            Some(Frame::Object {
                role,
                state: ObjectState::Value,
                pending_key,
                ..
            }) => {
                let target = pending_key
                    .as_ref()
                    .and_then(|key| artifact_target(*role, &key.value));
                if pending_key.as_ref().is_some_and(|key| key.duplicate) {
                    if let Some(target) = target {
                        push_artifact_reset(events, target);
                    }
                }
                StringPurpose::Value(target)
            }
            Some(Frame::Array {
                state: ArrayState::ValueOrEnd,
                ..
            }) => StringPurpose::Value(None),
            _ => {
                self.failed = true;
                return;
            }
        };
        self.string = Some(JsonStringState::new(purpose));
    }

    fn complete_string(&mut self, string: JsonStringState) {
        match string.purpose {
            StringPurpose::Key(value) => {
                let Some(Frame::Object {
                    state,
                    pending_key,
                    seen_keys,
                    ..
                }) = self.frames.last_mut()
                else {
                    self.failed = true;
                    return;
                };
                let duplicate = !seen_keys.insert(value.clone());
                *pending_key = Some(PendingKey { value, duplicate });
                *state = ObjectState::Colon;
            }
            StringPurpose::Value(_) => {
                if !self.mark_scalar_value_complete() {
                    self.failed = true;
                }
            }
        }
    }

    fn accept_colon(&mut self) {
        let Some(Frame::Object { state, .. }) = self.frames.last_mut() else {
            self.failed = true;
            return;
        };
        if *state != ObjectState::Colon {
            self.failed = true;
            return;
        }
        *state = ObjectState::Value;
    }

    fn accept_comma(&mut self) {
        match self.frames.last_mut() {
            Some(Frame::Object {
                state, pending_key, ..
            }) if *state == ObjectState::CommaOrEnd => {
                *state = ObjectState::KeyOrEnd;
                *pending_key = None;
            }
            Some(Frame::Array {
                state, item_index, ..
            }) if *state == ArrayState::CommaOrEnd => {
                *state = ArrayState::ValueOrEnd;
                *item_index = item_index.saturating_add(1);
            }
            _ => self.failed = true,
        }
    }

    fn start_object(&mut self) {
        let role = match self.frames.last() {
            Some(Frame::Array {
                role: ArrayRole::Findings,
                state: ArrayState::ValueOrEnd,
                item_index,
            }) => ObjectRole::Finding(*item_index),
            _ => ObjectRole::Other,
        };
        if !self.mark_container_value_started() {
            self.failed = true;
            return;
        }
        self.frames.push(Frame::object(role));
    }

    fn close_object(&mut self) {
        let valid = matches!(
            self.frames.last(),
            Some(Frame::Object {
                state: ObjectState::KeyOrEnd | ObjectState::CommaOrEnd,
                ..
            })
        );
        if !valid {
            self.failed = true;
            return;
        }
        self.frames.pop();
        if self.frames.is_empty() {
            self.root_complete = true;
        }
    }

    fn start_array(&mut self) {
        let role = match self.frames.last() {
            Some(Frame::Object {
                role: ObjectRole::Root,
                state: ObjectState::Value,
                pending_key: Some(key),
                ..
            }) if key.value == "findings" => ArrayRole::Findings,
            _ => ArrayRole::Other,
        };
        if !self.mark_container_value_started() {
            self.failed = true;
            return;
        }
        self.frames.push(Frame::Array {
            role,
            state: ArrayState::ValueOrEnd,
            item_index: 0,
        });
    }

    fn close_array(&mut self) {
        let valid = matches!(
            self.frames.last(),
            Some(Frame::Array {
                state: ArrayState::ValueOrEnd | ArrayState::CommaOrEnd,
                ..
            })
        );
        if !valid {
            self.failed = true;
            return;
        }
        self.frames.pop();
    }

    fn mark_container_value_started(&mut self) -> bool {
        match self.frames.last_mut() {
            Some(Frame::Object {
                state: state @ ObjectState::Value,
                ..
            }) => {
                *state = ObjectState::CommaOrEnd;
                true
            }
            Some(Frame::Array {
                state: state @ ArrayState::ValueOrEnd,
                ..
            }) => {
                *state = ArrayState::CommaOrEnd;
                true
            }
            _ => false,
        }
    }

    fn mark_scalar_value_complete(&mut self) -> bool {
        self.mark_container_value_started()
    }
}

#[derive(Debug)]
enum Frame {
    Object {
        role: ObjectRole,
        state: ObjectState,
        pending_key: Option<PendingKey>,
        seen_keys: HashSet<String>,
    },
    Array {
        role: ArrayRole,
        state: ArrayState,
        item_index: u32,
    },
}

impl Frame {
    fn object(role: ObjectRole) -> Self {
        Self::Object {
            role,
            state: ObjectState::KeyOrEnd,
            pending_key: None,
            seen_keys: HashSet::new(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ArtifactTarget {
    field: &'static str,
    item_index: Option<u32>,
}

#[derive(Debug, Clone, Copy)]
enum ObjectRole {
    Root,
    Finding(u32),
    Other,
}

#[derive(Debug, Clone, Copy)]
enum ArrayRole {
    Findings,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObjectState {
    KeyOrEnd,
    Colon,
    Value,
    CommaOrEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArrayState {
    ValueOrEnd,
    CommaOrEnd,
}

#[derive(Debug)]
struct PendingKey {
    value: String,
    duplicate: bool,
}

fn artifact_target(role: ObjectRole, key: &str) -> Option<ArtifactTarget> {
    match (role, key) {
        (ObjectRole::Root, "summary") => Some(ArtifactTarget {
            field: "summary",
            item_index: None,
        }),
        (ObjectRole::Finding(index), "title") => Some(ArtifactTarget {
            field: "finding_title",
            item_index: Some(index),
        }),
        (ObjectRole::Finding(index), "explanation") => Some(ArtifactTarget {
            field: "finding_explanation",
            item_index: Some(index),
        }),
        _ => None,
    }
}

struct JsonStringState {
    purpose: StringPurpose,
    escape: EscapeState,
}

impl JsonStringState {
    fn new(purpose: StringPurpose) -> Self {
        Self {
            purpose,
            escape: EscapeState::Normal,
        }
    }

    fn consume(&mut self, character: char) -> StringAction {
        match self.escape {
            EscapeState::Normal => match character {
                '"' => StringAction::Complete,
                '\\' => {
                    self.escape = EscapeState::AfterSlash;
                    StringAction::Continue(None)
                }
                value if value < '\u{20}' => StringAction::Failed,
                value => StringAction::Continue(Some(value)),
            },
            EscapeState::AfterSlash => {
                let decoded = match character {
                    '"' | '\\' | '/' => Some(character),
                    'b' => Some('\u{0008}'),
                    'f' => Some('\u{000c}'),
                    'n' => Some('\n'),
                    'r' => Some('\r'),
                    't' => Some('\t'),
                    'u' => {
                        self.escape = EscapeState::Unicode {
                            value: 0,
                            digits: 0,
                        };
                        return StringAction::Continue(None);
                    }
                    _ => return StringAction::Failed,
                };
                self.escape = EscapeState::Normal;
                StringAction::Continue(decoded)
            }
            EscapeState::Unicode { value, digits } => {
                let Some(digit) = character.to_digit(16) else {
                    return StringAction::Failed;
                };
                let value = (value << 4) | digit as u16;
                let digits = digits + 1;
                if digits < 4 {
                    self.escape = EscapeState::Unicode { value, digits };
                    return StringAction::Continue(None);
                }
                if (0xD800..=0xDBFF).contains(&value) {
                    self.escape = EscapeState::ExpectLowSlash { high: value };
                    return StringAction::Continue(None);
                }
                if (0xDC00..=0xDFFF).contains(&value) {
                    return StringAction::Failed;
                }
                self.escape = EscapeState::Normal;
                char::from_u32(u32::from(value)).map_or(StringAction::Failed, |decoded| {
                    StringAction::Continue(Some(decoded))
                })
            }
            EscapeState::ExpectLowSlash { high } => {
                if character != '\\' {
                    return StringAction::Failed;
                }
                self.escape = EscapeState::ExpectLowU { high };
                StringAction::Continue(None)
            }
            EscapeState::ExpectLowU { high } => {
                if character != 'u' {
                    return StringAction::Failed;
                }
                self.escape = EscapeState::LowUnicode {
                    high,
                    value: 0,
                    digits: 0,
                };
                StringAction::Continue(None)
            }
            EscapeState::LowUnicode {
                high,
                value,
                digits,
            } => {
                let Some(digit) = character.to_digit(16) else {
                    return StringAction::Failed;
                };
                let value = (value << 4) | digit as u16;
                let digits = digits + 1;
                if digits < 4 {
                    self.escape = EscapeState::LowUnicode {
                        high,
                        value,
                        digits,
                    };
                    return StringAction::Continue(None);
                }
                if !(0xDC00..=0xDFFF).contains(&value) {
                    return StringAction::Failed;
                }
                let codepoint =
                    0x1_0000 + ((u32::from(high) - 0xD800) << 10) + (u32::from(value) - 0xDC00);
                self.escape = EscapeState::Normal;
                char::from_u32(codepoint).map_or(StringAction::Failed, |decoded| {
                    StringAction::Continue(Some(decoded))
                })
            }
        }
    }

    fn push_decoded(&mut self, decoded: char, events: &mut Vec<AgentEventKind>) {
        match &mut self.purpose {
            StringPurpose::Key(value) => value.push(decoded),
            StringPurpose::Value(Some(target)) => push_artifact_delta(events, *target, decoded),
            StringPurpose::Value(None) => {}
        }
    }
}

enum StringPurpose {
    Key(String),
    Value(Option<ArtifactTarget>),
}

#[derive(Clone, Copy)]
enum EscapeState {
    Normal,
    AfterSlash,
    Unicode { value: u16, digits: u8 },
    ExpectLowSlash { high: u16 },
    ExpectLowU { high: u16 },
    LowUnicode { high: u16, value: u16, digits: u8 },
}

enum StringAction {
    Continue(Option<char>),
    Complete,
    Failed,
}

fn push_artifact_delta(events: &mut Vec<AgentEventKind>, target: ArtifactTarget, decoded: char) {
    if let Some(AgentEventKind::ArtifactTextDelta {
        artifact_type,
        field,
        item_index,
        delta,
    }) = events.last_mut()
    {
        if artifact_type == HISTORY_ARTIFACT_TYPE
            && field == target.field
            && *item_index == target.item_index
        {
            delta.push(decoded);
            return;
        }
    }
    events.push(AgentEventKind::ArtifactTextDelta {
        artifact_type: HISTORY_ARTIFACT_TYPE.into(),
        field: target.field.into(),
        item_index: target.item_index,
        delta: decoded.to_string(),
    });
}

fn push_artifact_reset(events: &mut Vec<AgentEventKind>, target: ArtifactTarget) {
    events.push(AgentEventKind::ArtifactTextReset {
        artifact_type: HISTORY_ARTIFACT_TYPE.into(),
        field: target.field.into(),
        item_index: target.item_index,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_structure_and_finding_index_when_fields_are_reordered() {
        let mut stream = HistoryArtifactStream::default();
        let events = stream.feed(
            r#"{"findings":[{"explanation":"Why","metadata":{"title":"ignore"},"title":"First"},{"title":"Second","explanation":"Two"}],"meta":{"summary":"ignore"},"summary":"Done"}"#,
        );
        assert_eq!(artifact_text(&events, "summary", None), "Done");
        assert_eq!(artifact_text(&events, "finding_title", Some(0)), "First");
        assert_eq!(
            artifact_text(&events, "finding_explanation", Some(0)),
            "Why"
        );
        assert_eq!(artifact_text(&events, "finding_title", Some(1)), "Second");
        assert_eq!(
            artifact_text(&events, "finding_explanation", Some(1)),
            "Two"
        );
        assert!(!format!("{events:?}").contains("ignore"));
    }

    #[test]
    fn combines_unicode_surrogate_pairs_split_across_chunks() {
        let mut stream = HistoryArtifactStream::default();
        let mut events = stream.feed(r#"{"summary":"Hi \uD83D"#);
        events.extend(stream.feed(r#"\uDE00!"}"#));
        assert_eq!(artifact_text(&events, "summary", None), "Hi 😀!");
    }

    #[test]
    fn duplicate_display_key_resets_before_streaming_the_last_value() {
        let mut stream = HistoryArtifactStream::default();
        let events = stream.feed(
            r#"{"summary":"Summary","findings":[{"title":"old","title":"new","explanation":"Why"}]}"#,
        );
        let title_events: Vec<_> = events
            .iter()
            .filter(|event| match event {
                AgentEventKind::ArtifactTextDelta { field, .. }
                | AgentEventKind::ArtifactTextReset { field, .. } => field == "finding_title",
                _ => false,
            })
            .collect();
        assert!(matches!(
            title_events[1],
            AgentEventKind::ArtifactTextReset { .. }
        ));
        assert_eq!(
            artifact_text_after_last_reset(&events, "finding_title", Some(0)),
            "new"
        );
    }

    #[test]
    fn keeps_interleaved_attempt_parsers_independent() {
        let augmenter = HistoryEventAugmenter::default();
        augmenter.additional_events(
            1,
            &AgentEventKind::ModelAttemptStarted {
                provider_id: "deepseek".into(),
                model_id: "deepseek-chat".into(),
            },
        );
        augmenter.additional_events(
            2,
            &AgentEventKind::ModelAttemptStarted {
                provider_id: "deepseek".into(),
                model_id: "deepseek-chat".into(),
            },
        );
        let first = augmenter.additional_events(
            1,
            &AgentEventKind::OutputTextDelta {
                delta: r#"{"summary":"One"#.into(),
            },
        );
        let second = augmenter.additional_events(
            2,
            &AgentEventKind::OutputTextDelta {
                delta: r#"{"summary":"Two"}"#.into(),
            },
        );
        let first_tail = augmenter.additional_events(
            1,
            &AgentEventKind::OutputTextDelta {
                delta: r#"!"}"#.into(),
            },
        );
        assert_eq!(artifact_text(&first, "summary", None), "One");
        assert_eq!(artifact_text(&second, "summary", None), "Two");
        assert_eq!(artifact_text(&first_tail, "summary", None), "!");
    }

    #[test]
    fn consumes_character_sized_chunks_without_retaining_the_raw_response() {
        let source = format!(r#"{{"summary":"{}"}}"#, "a".repeat(12_000));
        let mut stream = HistoryArtifactStream::default();
        let mut output = String::new();
        for character in source.chars() {
            let events = stream.feed(&character.to_string());
            output.push_str(&artifact_text(&events, "summary", None));
        }
        assert_eq!(output.len(), 12_000);
        assert!(stream.root_complete);
    }

    fn artifact_text(events: &[AgentEventKind], field: &str, item_index: Option<u32>) -> String {
        events
            .iter()
            .filter_map(|event| match event {
                AgentEventKind::ArtifactTextDelta {
                    field: candidate,
                    item_index: candidate_index,
                    delta,
                    ..
                } if candidate == field && *candidate_index == item_index => Some(delta.as_str()),
                _ => None,
            })
            .collect()
    }

    fn artifact_text_after_last_reset(
        events: &[AgentEventKind],
        field: &str,
        item_index: Option<u32>,
    ) -> String {
        let mut output = String::new();
        for event in events {
            match event {
                AgentEventKind::ArtifactTextReset {
                    field: candidate,
                    item_index: candidate_index,
                    ..
                } if candidate == field && *candidate_index == item_index => output.clear(),
                AgentEventKind::ArtifactTextDelta {
                    field: candidate,
                    item_index: candidate_index,
                    delta,
                    ..
                } if candidate == field && *candidate_index == item_index => output.push_str(delta),
                _ => {}
            }
        }
        output
    }
}
