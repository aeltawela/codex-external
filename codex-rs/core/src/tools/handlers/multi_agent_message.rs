use crate::context::ContextualUserFragment;
use crate::context::InterAgentMessage;
use crate::context::InterAgentMessageType;
use crate::function_tool::FunctionCallError;
use crate::tools::context::DirectToolCallSourcePolicy;
use crate::tools::context::ToolCallSource;
use codex_protocol::AgentPath;
use codex_protocol::protocol::InterAgentCommunication;
use codex_tools::JsonSchema;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolExposure;
use codex_tools::ToolSpec;
use std::collections::BTreeMap;

// Keep the full rendered inter-agent item below the repository's 10K-token hard cap,
// even for text that tokenizes close to one token per UTF-8 byte.
pub(crate) const MAX_PLAINTEXT_AGENT_MESSAGE_BYTES: usize = 8 * 1024;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum AgentMessageRoute {
    #[default]
    Native,
    ProviderPlaintext,
    ExternalPlaintext,
}

impl AgentMessageRoute {
    pub(crate) fn messaging_tool_exposure(self, configured: ToolExposure) -> ToolExposure {
        match self {
            // Code Mode cannot produce the encrypted arguments required by native V2 messaging.
            Self::Native => ToolExposure::DirectModelOnly,
            Self::ProviderPlaintext | Self::ExternalPlaintext => configured,
        }
    }

    pub(crate) fn direct_tool_call_source_policy(self) -> DirectToolCallSourcePolicy {
        match self {
            Self::ProviderPlaintext | Self::ExternalPlaintext => {
                DirectToolCallSourcePolicy::Plaintext
            }
            Self::Native => DirectToolCallSourcePolicy::EncryptedWithPlaintextFallback,
        }
    }

    pub(crate) fn requires_explicit_agent_type(self) -> bool {
        self == Self::ExternalPlaintext
    }

    pub(crate) fn accepts_spawn_model_overrides(self) -> bool {
        self != Self::ExternalPlaintext
    }

    pub(crate) fn validate_spawn_selection(
        self,
        agent_type: Option<&str>,
        has_model_override: bool,
        has_reasoning_effort_override: bool,
    ) -> Result<Option<&str>, FunctionCallError> {
        let role_name = agent_type.map(str::trim).filter(|role| !role.is_empty());
        if self.requires_explicit_agent_type() && role_name.is_none() {
            return Err(FunctionCallError::RespondToModel(
                "External agent spawning requires a non-empty agent_type".to_string(),
            ));
        }
        if !self.accepts_spawn_model_overrides()
            && (has_model_override || has_reasoning_effort_override)
        {
            return Err(FunctionCallError::RespondToModel(
                "External agent spawning uses the selected role's configured model and reasoning effort; omit model and reasoning_effort"
                    .to_string(),
            ));
        }
        Ok(role_name)
    }

    pub(crate) fn validate_message(self, message: &str) -> Result<(), FunctionCallError> {
        if message.trim().is_empty() {
            return Err(FunctionCallError::RespondToModel(
                "Empty message can't be sent to an agent".to_string(),
            ));
        }
        if self != Self::Native && message.len() > MAX_PLAINTEXT_AGENT_MESSAGE_BYTES {
            return Err(FunctionCallError::RespondToModel(format!(
                "Agent messages can't exceed {MAX_PLAINTEXT_AGENT_MESSAGE_BYTES} UTF-8 bytes"
            )));
        }
        Ok(())
    }

    fn validate_recipient_provider(
        self,
        provider_is_openai: bool,
    ) -> Result<(), FunctionCallError> {
        match (self, provider_is_openai) {
            (Self::Native, true)
            | (Self::ProviderPlaintext, _)
            | (Self::ExternalPlaintext, false) => Ok(()),
            (Self::Native, false) => Err(FunctionCallError::RespondToModel(
                "The selected agent uses a non-OpenAI model provider; retry with the corresponding `external_agents` collaboration tool."
                    .to_string(),
            )),
            (Self::ExternalPlaintext, true) => Err(FunctionCallError::RespondToModel(
                "The selected agent uses the OpenAI model provider; retry with the standard collaboration tool."
                    .to_string(),
            )),
        }
    }

    pub(crate) fn into_communication(
        self,
        source: &ToolCallSource,
        recipient_is_openai: bool,
        author: AgentPath,
        recipient: AgentPath,
        message: String,
        trigger_turn: bool,
    ) -> Result<InterAgentCommunication, FunctionCallError> {
        self.validate_message(&message)?;
        self.validate_recipient_provider(recipient_is_openai)?;

        match (self, source) {
            (Self::Native, ToolCallSource::Direct) => {
                Ok(InterAgentCommunication::new_encrypted(
                    author,
                    recipient,
                    Vec::new(),
                    message,
                    trigger_turn,
                ))
            }
            (Self::Native, ToolCallSource::DirectPlaintextMessage) => {
                Ok(plaintext_communication(
                    author,
                    recipient,
                    message,
                    trigger_turn,
                ))
            }
            (Self::Native, ToolCallSource::CodeMode { .. }) => {
                Err(FunctionCallError::RespondToModel(
                    "OpenAI collaboration messages require a direct encrypted tool call; retry with the standard collaboration tool outside Code Mode."
                        .to_string(),
                ))
            }
            (
                Self::ProviderPlaintext | Self::ExternalPlaintext,
                ToolCallSource::Direct
                | ToolCallSource::DirectPlaintextMessage
                | ToolCallSource::CodeMode { .. },
            ) => {
                let communication = plaintext_communication(
                    author,
                    recipient,
                    message,
                    trigger_turn,
                );
                if communication.content.len() > MAX_PLAINTEXT_AGENT_MESSAGE_BYTES {
                    return Err(FunctionCallError::RespondToModel(format!(
                        "Rendered agent messages can't exceed {MAX_PLAINTEXT_AGENT_MESSAGE_BYTES} UTF-8 bytes"
                    )));
                }
                Ok(communication)
            }
        }
    }
}

fn plaintext_communication(
    author: AgentPath,
    recipient: AgentPath,
    message: String,
    trigger_turn: bool,
) -> InterAgentCommunication {
    let message_type = if trigger_turn {
        InterAgentMessageType::NewTask
    } else {
        InterAgentMessageType::Message
    };
    let content =
        InterAgentMessage::new(message_type, recipient.clone(), author.clone(), message).render();
    InterAgentCommunication::new(author, recipient, Vec::new(), content, trigger_turn)
}

#[cfg(test)]
pub(crate) fn create_send_message_tool() -> ToolSpec {
    create_send_message_tool_for_route(AgentMessageRoute::Native)
}

pub(crate) fn create_send_message_tool_for_route(message_route: AgentMessageRoute) -> ToolSpec {
    let mut properties = BTreeMap::from([(
        "target".to_string(),
        JsonSchema::string(Some(
            "Relative or canonical task name to message (from spawn_agent).".to_string(),
        )),
    )]);
    insert_agent_message_properties(
        &mut properties,
        "Message text to queue on the target agent.",
        message_route,
    );

    ToolSpec::Function(ResponsesApiTool {
        name: "send_message".to_string(),
        description: "Send a message to an existing agent. The message will be delivered promptly. Does not trigger a new turn."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            message_required_fields("target"),
            Some(false.into()),
        ),
        output_schema: None,
    })
}

#[cfg(test)]
pub(crate) fn create_followup_task_tool() -> ToolSpec {
    create_followup_task_tool_for_route(AgentMessageRoute::Native)
}

pub(crate) fn create_followup_task_tool_for_route(message_route: AgentMessageRoute) -> ToolSpec {
    let mut properties = BTreeMap::from([(
        "target".to_string(),
        JsonSchema::string(Some(
            "Agent id or canonical task name to send a follow-up task to (from spawn_agent)."
                .to_string(),
        )),
    )]);
    insert_agent_message_properties(
        &mut properties,
        "Message text to send to the target agent.",
        message_route,
    );

    ToolSpec::Function(ResponsesApiTool {
        name: "followup_task".to_string(),
        description: "Send a follow-up task to an existing non-root target agent and trigger a turn if it is idle. If the target is already running, deliver the task promptly at message boundaries while sampling, or after the pending tool call completes."
            .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(
            properties,
            message_required_fields("target"),
            Some(false.into()),
        ),
        output_schema: None,
    })
}

pub(super) fn insert_agent_message_properties(
    properties: &mut BTreeMap<String, JsonSchema>,
    description: &str,
    message_route: AgentMessageRoute,
) {
    let message_description = match message_route {
        AgentMessageRoute::Native => description.to_string(),
        AgentMessageRoute::ProviderPlaintext => plaintext_message_description(description),
        AgentMessageRoute::ExternalPlaintext => plaintext_message_description(&format!(
            "{description} The target must use a configured non-OpenAI model provider."
        )),
    };
    let message = JsonSchema::string(Some(message_description));
    properties.insert(
        "message".to_string(),
        match message_route {
            AgentMessageRoute::Native => message.with_encrypted(),
            AgentMessageRoute::ProviderPlaintext | AgentMessageRoute::ExternalPlaintext => message,
        },
    );
}

pub(super) fn spawn_required_fields(message_route: AgentMessageRoute) -> Option<Vec<String>> {
    let mut required = vec!["task_name".to_string(), "message".to_string()];
    if message_route.requires_explicit_agent_type() {
        required.push("agent_type".to_string());
    }
    Some(required)
}

fn plaintext_message_description(description: &str) -> String {
    format!("{description} Maximum size: {MAX_PLAINTEXT_AGENT_MESSAGE_BYTES} UTF-8 bytes.")
}

fn message_required_fields(first: &str) -> Option<Vec<String>> {
    Some(vec![first.to_string(), "message".to_string()])
}

#[cfg(test)]
#[path = "multi_agent_message_tests.rs"]
mod tests;
