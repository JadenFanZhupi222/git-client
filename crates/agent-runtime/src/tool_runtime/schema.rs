use super::*;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ToolRegistrationError {
    #[error("invalid tool definition: {0}")]
    InvalidDefinition(&'static str),
    #[error("duplicate tool name")]
    DuplicateName,
    #[error("invalid tool input schema: {0}")]
    InvalidSchema(&'static str),
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ToolExecutionError {
    #[error("invalid tool call: {0}")]
    InvalidCall(&'static str),
    #[error("unknown tool")]
    UnknownTool,
    #[error("invalid tool input at {path}: {code}")]
    InvalidInput { path: String, code: &'static str },
    #[error("tool run budget exceeded: {0}")]
    BudgetExceeded(&'static str),
    #[error("tool run cancelled")]
    Cancelled,
    #[error("tool execution timed out")]
    Timeout,
    #[error("tool intent persistence failed")]
    IntentPersistence,
    #[error("tool receipt persistence failed")]
    ReceiptPersistence,
    #[error("tool intent resolution persistence failed")]
    IntentResolutionPersistence,
    #[error("tool intent preparation failed")]
    IntentPreparation,
}

#[derive(Clone)]
pub(super) struct RegisteredTool {
    pub(super) definition: crate::ToolDefinition,
    pub(super) schema: CompiledSchema,
    pub(super) handler: Arc<dyn ToolHandler>,
}

#[derive(Default)]
pub struct ToolRegistry {
    entries: HashMap<String, RegisteredTool>,
}

impl ToolRegistry {
    pub fn register(
        &mut self,
        definition: crate::ToolDefinition,
        handler: Arc<dyn ToolHandler>,
    ) -> Result<(), ToolRegistrationError> {
        validate_definition(&definition)?;
        if self.entries.contains_key(&definition.name) {
            return Err(ToolRegistrationError::DuplicateName);
        }
        let schema = CompiledSchema::compile(&definition.input_schema)?;
        self.entries.insert(
            definition.name.clone(),
            RegisteredTool {
                definition,
                schema,
                handler,
            },
        );
        Ok(())
    }

    pub fn definitions(&self) -> Vec<crate::ToolDefinition> {
        let mut definitions = self
            .entries
            .values()
            .map(|entry| entry.definition.clone())
            .collect::<Vec<_>>();
        definitions.sort_by(|left, right| left.name.cmp(&right.name));
        definitions
    }

    pub(super) fn get(&self, name: &str) -> Option<&RegisteredTool> {
        self.entries.get(name)
    }
}

fn validate_definition(definition: &crate::ToolDefinition) -> Result<(), ToolRegistrationError> {
    let valid_name = !definition.name.is_empty()
        && definition.name.len() <= 128
        && definition.name.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.' | b':' | b'/')
        });
    if !valid_name {
        return Err(ToolRegistrationError::InvalidDefinition("name"));
    }
    if definition.description.trim().is_empty() || definition.description.len() > 4096 {
        return Err(ToolRegistrationError::InvalidDefinition("description"));
    }
    if definition.timeout_ms == 0 {
        return Err(ToolRegistrationError::InvalidDefinition("timeout"));
    }
    if definition.max_result_bytes == 0 || definition.max_result_bytes > HARD_MAX_RESULT_BYTES {
        return Err(ToolRegistrationError::InvalidDefinition("result_limit"));
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(super) struct CompiledSchema(Value);

impl CompiledSchema {
    pub(super) fn compile(schema: &Value) -> Result<Self, ToolRegistrationError> {
        audit_schema(schema)?;
        Ok(Self(schema.clone()))
    }

    pub(super) fn validate(&self, value: &Value) -> Result<(), ToolExecutionError> {
        validate_value(&self.0, value, "$")
    }
}

fn audit_schema(schema: &Value) -> Result<(), ToolRegistrationError> {
    let object = schema
        .as_object()
        .ok_or(ToolRegistrationError::InvalidSchema("schema_not_object"))?;
    const ALLOWED: &[&str] = &[
        "$schema",
        "title",
        "description",
        "type",
        "properties",
        "required",
        "additionalProperties",
        "minProperties",
        "maxProperties",
        "items",
        "minItems",
        "maxItems",
        "uniqueItems",
        "minLength",
        "maxLength",
        "pattern",
        "minimum",
        "maximum",
        "exclusiveMinimum",
        "exclusiveMaximum",
        "multipleOf",
        "enum",
        "const",
        "oneOf",
    ];
    if object.keys().any(|key| !ALLOWED.contains(&key.as_str())) {
        return Err(ToolRegistrationError::InvalidSchema("unsupported_keyword"));
    }
    if let Some(kind) = object.get("type") {
        let Some(kind) = kind.as_str() else {
            return Err(ToolRegistrationError::InvalidSchema("invalid_type"));
        };
        if !matches!(
            kind,
            "object" | "array" | "string" | "integer" | "number" | "boolean" | "null"
        ) {
            return Err(ToolRegistrationError::InvalidSchema("unsupported_type"));
        }
    }
    for annotation in ["$schema", "title", "description"] {
        if object
            .get(annotation)
            .is_some_and(|value| !value.is_string())
        {
            return Err(ToolRegistrationError::InvalidSchema("invalid_annotation"));
        }
    }
    if object.contains_key("$ref") {
        return Err(ToolRegistrationError::InvalidSchema("external_reference"));
    }
    if let Some(properties) = object.get("properties") {
        let properties = properties
            .as_object()
            .ok_or(ToolRegistrationError::InvalidSchema("invalid_properties"))?;
        for child in properties.values() {
            audit_schema(child)?;
        }
    }
    if let Some(required) = object.get("required") {
        let required = required
            .as_array()
            .ok_or(ToolRegistrationError::InvalidSchema("invalid_required"))?;
        let mut names = HashSet::new();
        for name in required {
            let name = name
                .as_str()
                .ok_or(ToolRegistrationError::InvalidSchema("invalid_required"))?;
            if !names.insert(name) {
                return Err(ToolRegistrationError::InvalidSchema("duplicate_required"));
            }
        }
    }
    if object
        .get("additionalProperties")
        .is_some_and(|value| !value.is_boolean())
    {
        return Err(ToolRegistrationError::InvalidSchema(
            "invalid_additional_properties",
        ));
    }
    for key in [
        "minProperties",
        "maxProperties",
        "minItems",
        "maxItems",
        "minLength",
        "maxLength",
    ] {
        if object
            .get(key)
            .is_some_and(|value| value.as_u64().is_none())
        {
            return Err(ToolRegistrationError::InvalidSchema(
                "invalid_unsigned_limit",
            ));
        }
    }
    for (min, max) in [
        ("minProperties", "maxProperties"),
        ("minItems", "maxItems"),
        ("minLength", "maxLength"),
    ] {
        if let (Some(min), Some(max)) = (
            object.get(min).and_then(Value::as_u64),
            object.get(max).and_then(Value::as_u64),
        ) {
            if min > max {
                return Err(ToolRegistrationError::InvalidSchema("contradictory_limits"));
            }
        }
    }
    if object
        .get("uniqueItems")
        .is_some_and(|value| !value.is_boolean())
    {
        return Err(ToolRegistrationError::InvalidSchema("invalid_unique_items"));
    }
    if let Some(items) = object.get("items") {
        audit_schema(items)?;
    }
    if let Some(pattern) = object.get("pattern") {
        let pattern = pattern
            .as_str()
            .ok_or(ToolRegistrationError::InvalidSchema("invalid_pattern"))?;
        Regex::new(pattern).map_err(|_| ToolRegistrationError::InvalidSchema("invalid_pattern"))?;
    }
    for key in ["minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum"] {
        if object
            .get(key)
            .is_some_and(|value| value.as_f64().is_none())
        {
            return Err(ToolRegistrationError::InvalidSchema("invalid_number_limit"));
        }
    }
    if object
        .get("multipleOf")
        .is_some_and(|value| value.as_f64().is_none_or(|number| number <= 0.0))
    {
        return Err(ToolRegistrationError::InvalidSchema("invalid_multiple_of"));
    }
    if object
        .get("enum")
        .is_some_and(|value| value.as_array().is_none_or(Vec::is_empty))
    {
        return Err(ToolRegistrationError::InvalidSchema("invalid_enum"));
    }
    if let Some(one_of) = object.get("oneOf") {
        let one_of = one_of
            .as_array()
            .filter(|choices| !choices.is_empty())
            .ok_or(ToolRegistrationError::InvalidSchema("invalid_one_of"))?;
        for child in one_of {
            audit_schema(child)?;
        }
    }
    Ok(())
}

fn invalid(path: &str, code: &'static str) -> ToolExecutionError {
    ToolExecutionError::InvalidInput {
        path: path.chars().take(256).collect(),
        code,
    }
}

fn validate_value(schema: &Value, value: &Value, path: &str) -> Result<(), ToolExecutionError> {
    let schema = schema.as_object().expect("schema audited at registration");
    if let Some(expected) = schema.get("const") {
        if value != expected {
            return Err(invalid(path, "const"));
        }
    }
    if let Some(choices) = schema.get("enum").and_then(Value::as_array) {
        if !choices.contains(value) {
            return Err(invalid(path, "enum"));
        }
    }
    if let Some(choices) = schema.get("oneOf").and_then(Value::as_array) {
        let matches = choices
            .iter()
            .filter(|choice| validate_value(choice, value, path).is_ok())
            .count();
        if matches != 1 {
            return Err(invalid(path, "one_of"));
        }
    }
    if let Some(kind) = schema.get("type").and_then(Value::as_str) {
        let valid = match kind {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            "number" => value.is_number(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            _ => false,
        };
        if !valid {
            return Err(invalid(path, "type"));
        }
    }
    if let Some(object) = value.as_object() {
        let properties = schema.get("properties").and_then(Value::as_object);
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for name in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(name) {
                    return Err(invalid(path, "required"));
                }
            }
        }
        let additional = schema
            .get("additionalProperties")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        for (name, child) in object {
            match properties.and_then(|properties| properties.get(name)) {
                Some(child_schema) => {
                    validate_value(child_schema, child, &format!("{path}.{name}"))?
                }
                None if !additional => return Err(invalid(path, "additional_property")),
                None => {}
            }
        }
        let len = object.len() as u64;
        if schema
            .get("minProperties")
            .and_then(Value::as_u64)
            .is_some_and(|min| len < min)
        {
            return Err(invalid(path, "min_properties"));
        }
        if schema
            .get("maxProperties")
            .and_then(Value::as_u64)
            .is_some_and(|max| len > max)
        {
            return Err(invalid(path, "max_properties"));
        }
    }
    if let Some(array) = value.as_array() {
        let len = array.len() as u64;
        if schema
            .get("minItems")
            .and_then(Value::as_u64)
            .is_some_and(|min| len < min)
        {
            return Err(invalid(path, "min_items"));
        }
        if schema
            .get("maxItems")
            .and_then(Value::as_u64)
            .is_some_and(|max| len > max)
        {
            return Err(invalid(path, "max_items"));
        }
        if schema.get("uniqueItems").and_then(Value::as_bool) == Some(true) {
            for (index, item) in array.iter().enumerate() {
                if array[..index].contains(item) {
                    return Err(invalid(path, "unique_items"));
                }
            }
        }
        if let Some(items) = schema.get("items") {
            for (index, item) in array.iter().enumerate() {
                validate_value(items, item, &format!("{path}[{index}]"))?;
            }
        }
    }
    if let Some(text) = value.as_str() {
        let len = text.chars().count() as u64;
        if schema
            .get("minLength")
            .and_then(Value::as_u64)
            .is_some_and(|min| len < min)
        {
            return Err(invalid(path, "min_length"));
        }
        if schema
            .get("maxLength")
            .and_then(Value::as_u64)
            .is_some_and(|max| len > max)
        {
            return Err(invalid(path, "max_length"));
        }
        if let Some(pattern) = schema.get("pattern").and_then(Value::as_str) {
            if !Regex::new(pattern).expect("pattern audited").is_match(text) {
                return Err(invalid(path, "pattern"));
            }
        }
    }
    if let Some(number) = value.as_f64() {
        if schema
            .get("minimum")
            .and_then(Value::as_f64)
            .is_some_and(|min| number < min)
            || schema
                .get("exclusiveMinimum")
                .and_then(Value::as_f64)
                .is_some_and(|min| number <= min)
            || schema
                .get("maximum")
                .and_then(Value::as_f64)
                .is_some_and(|max| number > max)
            || schema
                .get("exclusiveMaximum")
                .and_then(Value::as_f64)
                .is_some_and(|max| number >= max)
        {
            return Err(invalid(path, "number_range"));
        }
        if let Some(multiple) = schema.get("multipleOf").and_then(Value::as_f64) {
            let quotient = number / multiple;
            if (quotient - quotient.round()).abs() > f64::EPSILON * quotient.abs().max(1.0) * 8.0 {
                return Err(invalid(path, "multiple_of"));
            }
        }
    }
    Ok(())
}
