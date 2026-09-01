use super::Config;
use anyhow::anyhow;
use anyhow::ensure;
use codex_config::ConfigLayerSource;
use codex_model_provider_info::ModelProviderInfo;
use toml::Value as TomlValue;

const SUBAGENT_MODEL_PROVIDER_ALLOWLIST_KEY: &str = "subagent_model_provider_allowlist";

pub(crate) struct SubagentModelProviderBaseline {
    id: String,
    provider: ModelProviderInfo,
}

impl Config {
    pub(crate) fn subagent_model_provider_baseline(&self) -> SubagentModelProviderBaseline {
        SubagentModelProviderBaseline {
            id: self.model_provider_id.clone(),
            provider: self.model_provider.clone(),
        }
    }

    pub(crate) fn subagent_model_provider_baseline_for(
        &self,
        provider_id: &str,
    ) -> anyhow::Result<SubagentModelProviderBaseline> {
        let baseline = self.subagent_model_provider_baseline();
        let provider = baseline.resolve(self, provider_id)?;
        Ok(SubagentModelProviderBaseline {
            id: provider_id.to_string(),
            provider,
        })
    }

    pub(crate) fn select_subagent_model_provider(
        &mut self,
        baseline: &SubagentModelProviderBaseline,
        provider_id: &str,
    ) -> anyhow::Result<()> {
        let provider = baseline.resolve(self, provider_id)?;
        let provider_changed =
            self.model_provider_id != provider_id || self.model_provider != provider;
        self.model_provider_id = provider_id.to_string();
        self.model_provider = provider;
        if provider_changed {
            self.model_catalog = None;
        }
        Ok(())
    }

    pub(crate) fn has_authorized_external_subagent_role(&self) -> bool {
        let Some(allowlist) = self.user_subagent_model_provider_allowlist() else {
            return false;
        };
        self.agent_roles.values().any(|role| {
            role.model_provider.as_deref().is_some_and(|provider_id| {
                allowlist.iter().any(|id| id.as_str() == Some(provider_id))
                    && self
                        .model_providers
                        .get(provider_id)
                        .is_some_and(|provider| !provider.is_openai())
            })
        })
    }

    fn user_allows_subagent_model_provider(&self, provider_id: &str) -> bool {
        self.user_subagent_model_provider_allowlist()
            .is_some_and(|ids| ids.iter().any(|id| id.as_str() == Some(provider_id)))
    }

    fn user_subagent_model_provider_allowlist(&self) -> Option<&[TomlValue]> {
        self.config_layer_stack
            .layers_high_to_low()
            .filter(|layer| matches!(layer.name, ConfigLayerSource::User { .. }))
            .find_map(|layer| layer.config.get(SUBAGENT_MODEL_PROVIDER_ALLOWLIST_KEY))
            .and_then(TomlValue::as_array)
            .map(Vec::as_slice)
    }
}

impl SubagentModelProviderBaseline {
    fn resolve(&self, config: &Config, provider_id: &str) -> anyhow::Result<ModelProviderInfo> {
        if provider_id == self.id {
            return Ok(self.provider.clone());
        }

        ensure!(
            config.user_allows_subagent_model_provider(provider_id),
            "the provider is not in the current user-owned subagent allowlist"
        );

        config
            .model_providers
            .get(provider_id)
            .cloned()
            .ok_or_else(|| anyhow!("the provider is not configured by the current parent"))
    }
}

#[cfg(test)]
#[path = "subagent_model_provider_tests.rs"]
mod tests;
