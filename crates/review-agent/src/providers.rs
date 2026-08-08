use crate::{
    anthropic_model_catalog, deepseek_model_catalog, openai_model_catalog, AnthropicProvider,
    DeepSeekProvider, ModelCatalogEntry, ModelProvider, OpenAiProvider, ReviewError,
    CLAUDE_OPUS_5_MODEL, CLAUDE_SONNET_5_MODEL, DEEPSEEK_V4_FLASH_MODEL, DEEPSEEK_V4_PRO_MODEL,
    GPT_5_6_LUNA_MODEL, GPT_5_6_SOL_MODEL, GPT_5_6_TERRA_MODEL,
};

pub fn model_catalog() -> Vec<ModelCatalogEntry> {
    let mut catalog = deepseek_model_catalog();
    catalog.extend(openai_model_catalog());
    catalog.extend(anthropic_model_catalog());
    catalog
}

pub fn model_provider_id(model_id: &str) -> Option<&'static str> {
    match model_id {
        DEEPSEEK_V4_FLASH_MODEL | DEEPSEEK_V4_PRO_MODEL => Some("deepseek"),
        GPT_5_6_SOL_MODEL | GPT_5_6_TERRA_MODEL | GPT_5_6_LUNA_MODEL => Some("openai"),
        CLAUDE_SONNET_5_MODEL | CLAUDE_OPUS_5_MODEL => Some("anthropic"),
        _ => None,
    }
}

pub fn create_model_provider(
    api_key: impl Into<String>,
    model_id: &str,
) -> Result<Box<dyn ModelProvider>, ReviewError> {
    let api_key = api_key.into();
    match model_provider_id(model_id) {
        Some("deepseek") => Ok(Box::new(DeepSeekProvider::new_with_model(
            api_key, model_id,
        )?)),
        Some("openai") => Ok(Box::new(OpenAiProvider::new_with_model(api_key, model_id)?)),
        Some("anthropic") => Ok(Box::new(AnthropicProvider::new_with_model(
            api_key, model_id,
        )?)),
        _ => Err(ReviewError::InvalidModelOutput(
            "unsupported model provider".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn aggregate_catalog_has_unique_provider_qualified_models() {
        let catalog = model_catalog();
        assert_eq!(catalog.len(), 7);
        let ids = catalog
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<HashSet<_>>();
        assert_eq!(ids.len(), catalog.len());
        for entry in catalog {
            assert_eq!(
                model_provider_id(&entry.id),
                Some(entry.provider_id.as_str())
            );
        }
    }

    #[test]
    fn factory_builds_every_allowlisted_model_and_rejects_unknown_ids() {
        for entry in model_catalog() {
            let provider = create_model_provider("fixture-key", &entry.id).unwrap();
            assert_eq!(provider.descriptor().provider_id, entry.provider_id);
            assert_eq!(provider.descriptor().model_id, entry.id);
        }
        assert!(create_model_provider("fixture-key", "user-controlled-model").is_err());
    }
}
