//! Shared argument parsing and dispatch for the v2 agent messaging tools.
//!
//! `send_message` and `followup_task` share the same submission path and differ only in whether the
//! resulting `InterAgentCommunication` should wake the target immediately.

use super::analytics::ToolCallAnalytics;
use super::*;
use crate::agent::control::MessageDeliveryError;
use crate::agent::control::MessageDeliveryMode;
use crate::agent::child_config::build_agent_resume_config;
use crate::agent::control::AgentMessage;
use crate::tools::context::FunctionToolOutput;
use crate::tools::handlers::multi_agent_message::AgentMessageRoute;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
/// Input for the MultiAgentV2 `send_message` tool.
pub(crate) struct SendMessageArgs {
    pub(crate) target: String,
    pub(crate) message: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
/// Input for the MultiAgentV2 `followup_task` tool.
pub(crate) struct FollowupTaskArgs {
    pub(crate) target: String,
    pub(crate) message: String,
}

/// Handles the shared MultiAgentV2 message flow for both `send_message` and `followup_task`.
pub(super) async fn handle_message_string_tool(
    invocation: ToolInvocation,
    mode: MessageDeliveryMode,
    message_route: AgentMessageRoute,
    target: String,
    message: String,
    analytics: &mut ToolCallAnalytics,
) -> Result<FunctionToolOutput, FunctionCallError> {
    let ToolInvocation {
        session,
        turn,
        call_id,
        source,
        ..
    } = invocation;
    message_route.validate_message(&message)?;
    let receiver_thread_id = resolve_agent_target(&session, &turn, &target).await?;
    analytics.set_receiver(receiver_thread_id);
    let resume_config =
        build_agent_resume_config(&turn).map_err(FunctionCallError::RespondToModel)?;
    let receiver_is_openai = session.services.agent_control
        .agent_uses_openai_model_provider(receiver_thread_id, &resume_config)
        .await
        .map_err(|err| collab_agent_error(receiver_thread_id, err))?;
    let receiver = session.services.agent_control.ensure_agent_known(receiver_thread_id)
        .map_err(|err| collab_agent_error(receiver_thread_id, err))?;
    let receiver_path = receiver.agent_path.ok_or_else(|| {
        FunctionCallError::RespondToModel("target agent is missing an agent_path".to_string())
    })?;
    // Validate provider ownership and the rendered message bound before delivery.
    message_route.into_communication(
        &source,
        receiver_is_openai,
        turn.session_source.get_agent_path().unwrap_or_else(AgentPath::root),
        receiver_path,
        message.clone(),
        mode == MessageDeliveryMode::TriggerTurn,
    )?;
    let agent_message = if message_route == AgentMessageRoute::Native {
        agent_message_from_tool(message, &source)
    } else {
        AgentMessage::Plaintext(message)
    };
    let receiver_agent_path = session
        .services
        .agent_control
        .deliver_message(
            session.thread_id,
            &turn,
            receiver_thread_id,
            agent_message,
            mode,
        )
        .await
        .map_err(|err| match err {
            MessageDeliveryError::InvalidRequest(message) => {
                FunctionCallError::RespondToModel(message)
            }
            MessageDeliveryError::Agent(err) => collab_agent_error(receiver_thread_id, err),
        })?;
    emit_sub_agent_activity(
        &session,
        &turn,
        SubAgentActivityItem {
            id: call_id,
            agent_thread_id: receiver_thread_id,
            agent_path: receiver_agent_path,
            kind: SubAgentActivityKind::Interacted,
        },
    )
    .await;

    Ok(FunctionToolOutput::from_text(String::new(), Some(true)))
}
