use crate::{ReviewError, ReviewFinding};
use serde_json::Value;

pub const REVIEW_OUTPUT_CONTRACT: &str = r#"Return only one JSON object with exactly this shape: {"summary":"...","findings":[{"id":"unique-id","severity":"high|medium|low","path":"repository/relative/path","side":"LEFT|RIGHT","line":123,"title":"...","failure_scenario":"...","explanation":"...","draft_comment":"..."}]}. Use an empty findings array when there are no actionable issues. Every finding must include all nine fields and must be tied to a selected patch line."#;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedReviewOutput {
    pub summary: String,
    pub findings: Vec<ReviewFinding>,
    pub structured: bool,
}

pub struct ReviewOutputCodec;

impl ReviewOutputCodec {
    pub fn decode(text: &str) -> Result<DecodedReviewOutput, ReviewError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(ReviewError::InvalidModelOutput(
                "no tool calls or final output".into(),
            ));
        }
        let Ok(parsed) = parse_structured_output(text) else {
            return Ok(DecodedReviewOutput {
                summary: bounded_plain_text_summary(text),
                findings: Vec::new(),
                structured: false,
            });
        };
        let summary = parsed
            .get("summary")
            .and_then(Value::as_str)
            .filter(|summary| !summary.trim().is_empty())
            .ok_or_else(|| ReviewError::InvalidModelOutput("summary missing".into()))?
            .to_string();
        let finding_values: Vec<&Value> = match parsed.get("findings") {
            Some(Value::Array(findings)) => findings.iter().collect(),
            Some(Value::Object(_)) => vec![&parsed["findings"]],
            Some(Value::Null) => Vec::new(),
            Some(_) => {
                return Err(ReviewError::InvalidModelOutput(
                    "findings schema mismatch".into(),
                ));
            }
            None => return Err(ReviewError::InvalidModelOutput("findings missing".into())),
        };
        let findings = finding_values
            .iter()
            .filter_map(|finding| serde_json::from_value::<ReviewFinding>((*finding).clone()).ok())
            .collect();
        Ok(DecodedReviewOutput {
            summary,
            findings,
            structured: true,
        })
    }
}

fn bounded_plain_text_summary(text: &str) -> String {
    const MAX_SUMMARY_CHARS: usize = 4_000;
    let trimmed = text.trim();
    let mut summary: String = trimmed.chars().take(MAX_SUMMARY_CHARS).collect();
    if trimmed.chars().count() > MAX_SUMMARY_CHARS {
        summary.push('…');
    }
    summary
}

fn parse_structured_output(text: &str) -> Result<Value, ReviewError> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Ok(normalize_structured_output(value));
    }

    let without_fence = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```JSON"))
        .or_else(|| trimmed.strip_prefix("```"))
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim);
    if let Some(value) = without_fence {
        if let Ok(parsed) = serde_json::from_str(value) {
            return Ok(normalize_structured_output(parsed));
        }
    }

    if let Some(end) = trimmed.rfind('}') {
        for (start, _) in trimmed[..end].match_indices('{').rev() {
            if let Ok(parsed) = serde_json::from_str::<Value>(&trimmed[start..=end]) {
                let normalized = normalize_structured_output(parsed);
                if normalized.get("summary").is_some() && normalized.get("findings").is_some() {
                    return Ok(normalized);
                }
            }
        }
    }
    Err(ReviewError::InvalidModelOutput(
        "structured output was invalid".into(),
    ))
}

fn normalize_structured_output(mut value: Value) -> Value {
    if let Some(findings) = value.get_mut("findings").and_then(Value::as_array_mut) {
        for finding in findings {
            if let Some(object) = finding.as_object_mut() {
                if let Some(severity) = object
                    .get("severity")
                    .and_then(Value::as_str)
                    .map(str::to_ascii_lowercase)
                {
                    object.insert("severity".into(), Value::String(severity));
                }
                if let Some(side) = object
                    .get("side")
                    .and_then(Value::as_str)
                    .map(str::to_ascii_uppercase)
                {
                    object.insert("side".into(), Value::String(side));
                }
            }
        }
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decodes_structured_output_and_normalizes_enum_case() {
        let finding = json!({"id":"f","severity":"HIGH","path":"src/lib.rs","side":"right","line":1,"title":"t","failure_scenario":"s","explanation":"e","draft_comment":"d"});
        let decoded = ReviewOutputCodec::decode(&format!(
            "```json\n{}\n```",
            json!({"summary":"Issue","findings":[finding]})
        ))
        .unwrap();
        assert!(decoded.structured);
        assert_eq!(decoded.findings.len(), 1);
    }

    #[test]
    fn extracts_final_json_after_analysis_with_code_braces() {
        let decoded = ReviewOutputCodec::decode(
            "The callback {(kind) => open(kind)} looks fine.\n\n{\"summary\":\"No issue found.\",\"findings\":[]}",
        )
        .unwrap();
        assert!(decoded.structured);
        assert_eq!(decoded.summary, "No issue found.");
    }

    #[test]
    fn preserves_nonempty_plain_text_as_an_unstructured_review() {
        let decoded = ReviewOutputCodec::decode(
            "I reviewed the selected patch. No actionable correctness issue was found.",
        )
        .unwrap();
        assert!(!decoded.structured);
        assert!(decoded.summary.contains("No actionable"));
        assert!(decoded.findings.is_empty());
    }

    #[test]
    fn keeps_valid_findings_when_another_finding_is_malformed() {
        let valid = json!({"id":"f","severity":"high","path":"src/lib.rs","side":"RIGHT","line":1,"title":"t","failure_scenario":"s","explanation":"e","draft_comment":"d"});
        let decoded = ReviewOutputCodec::decode(
            &json!({"summary":"Found one valid issue.","findings":[{"title":"incomplete"},valid]})
                .to_string(),
        )
        .unwrap();
        assert_eq!(decoded.findings.len(), 1);
    }

    #[test]
    fn bounds_unstructured_unicode_without_splitting_characters() {
        let decoded = ReviewOutputCodec::decode(&"评".repeat(4_001)).unwrap();
        assert_eq!(decoded.summary.chars().count(), 4_001);
        assert!(decoded.summary.ends_with('…'));
    }
}
