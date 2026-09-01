use super::*;
use crate::config::SubagentModelProviderBaseline;

impl AgentControl {
    pub(crate) async fn agent_uses_openai_model_provider(
        &self,
        agent_id: ThreadId,
        config: &Config,
    ) -> CodexResult<bool> {
        let state = self.upgrade()?;
        if let Ok(thread) = state.get_thread(agent_id).await {
            return Ok(thread.session.provider().await.is_openai());
        }
        let stored_thread = state
            .read_stored_thread(ReadThreadParams {
                thread_id: agent_id,
                include_archived: true,
                include_history: false,
            })
            .await?;
        let parent_thread_id = stored_thread
            .parent_thread_id
            .or_else(|| stored_thread.source.parent_thread_id());
        let baseline = model_provider_baseline_for_parent(&state, config, parent_thread_id).await?;
        let mut resolved_config = config.clone();
        restore_persisted_agent_model_provider(
            &mut resolved_config,
            &baseline,
            &stored_thread.model_provider,
        )?;
        Ok(resolved_config.model_provider.is_openai())
    }
}

pub(super) fn restore_persisted_agent_model_provider(
    config: &mut Config,
    model_provider_baseline: &SubagentModelProviderBaseline,
    model_provider_id: &str,
) -> CodexResult<()> {
    config
        .select_subagent_model_provider(model_provider_baseline, model_provider_id)
        .map_err(|err| {
            CodexErr::InvalidRequest(format!(
                "cannot resume subagent with model provider `{model_provider_id}`: {err}"
            ))
        })?;
    Ok(())
}

pub(super) fn restore_persisted_agent_model(config: &mut Config, model: Option<String>) {
    if let Some(model) = model {
        config.model = Some(model);
    }
}

pub(super) async fn validate_history_fork_model_provider(
    state: &ThreadManagerState,
    config: &Config,
    session_source: Option<&SessionSource>,
    fork_mode: Option<&SpawnAgentForkMode>,
) -> CodexResult<()> {
    let Some((
        _,
        SessionSource::SubAgent(SubAgentSource::ThreadSpawn {
            parent_thread_id, ..
        }),
    )) = fork_mode.zip(session_source)
    else {
        return Ok(());
    };
    let parent = state.get_thread(*parent_thread_id).await?;
    let parent_config = parent.session.get_config().await;
    if config.model_provider_id != parent_config.model_provider_id
        || config.model_provider != parent_config.model_provider
    {
        return Err(CodexErr::InvalidRequest(
            "history forks require the child to use the parent's exact model provider configuration; use fork_turns=\"none\" for a different provider"
                .to_string(),
        ));
    }
    Ok(())
}

pub(super) async fn model_provider_baseline_for_parent(
    state: &ThreadManagerState,
    config: &Config,
    parent_thread_id: Option<ThreadId>,
) -> CodexResult<SubagentModelProviderBaseline> {
    let Some(parent_thread_id) = parent_thread_id else {
        return Ok(config.subagent_model_provider_baseline());
    };
    if let Ok(parent) = state.get_thread(parent_thread_id).await {
        return Ok(parent
            .session
            .get_config()
            .await
            .subagent_model_provider_baseline());
    }
    let parent = state
        .read_stored_thread(ReadThreadParams {
            thread_id: parent_thread_id,
            include_archived: true,
            include_history: false,
        })
        .await?;
    config
        .subagent_model_provider_baseline_for(&parent.model_provider)
        .map_err(|err| {
            CodexErr::InvalidRequest(format!(
                "cannot resolve the recorded parent model provider: {err}"
            ))
        })
}
