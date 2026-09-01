use super::*;

fn worker_path() -> AgentPath {
    AgentPath::root().join("worker").expect("valid worker path")
}

fn communication(
    route: AgentMessageRoute,
    source: ToolCallSource,
    recipient_is_openai: bool,
    message: String,
) -> Result<InterAgentCommunication, FunctionCallError> {
    route.into_communication(
        &source,
        recipient_is_openai,
        AgentPath::root(),
        worker_path(),
        message,
        /*trigger_turn*/ true,
    )
}

#[test]
fn route_owns_direct_call_argument_transport() {
    let encrypted_fields = ["message".to_string()];
    assert_eq!(
        AgentMessageRoute::Native
            .direct_tool_call_source_policy()
            .classify(Some(&encrypted_fields)),
        ToolCallSource::Direct
    );
    assert_eq!(
        AgentMessageRoute::Native
            .direct_tool_call_source_policy()
            .classify(Some(&[])),
        ToolCallSource::DirectPlaintextMessage
    );
    for route in [
        AgentMessageRoute::ProviderPlaintext,
        AgentMessageRoute::ExternalPlaintext,
    ] {
        assert_eq!(
            route
                .direct_tool_call_source_policy()
                .classify(Some(&encrypted_fields)),
            ToolCallSource::DirectPlaintextMessage
        );
    }
    assert_eq!(
        DirectToolCallSourcePolicy::default().classify(Some(&[])),
        ToolCallSource::Direct
    );
}

#[test]
fn external_spawn_selection_is_role_owned() {
    let route = AgentMessageRoute::ExternalPlaintext;

    assert!(route.validate_spawn_selection(None, false, false).is_err());
    assert!(
        route
            .validate_spawn_selection(Some("worker"), true, false)
            .is_err()
    );
    assert!(
        route
            .validate_spawn_selection(Some("worker"), false, true)
            .is_err()
    );
    assert_eq!(
        route
            .validate_spawn_selection(Some(" worker "), false, false)
            .expect("a configured external role should be accepted"),
        Some("worker")
    );
}

#[test]
fn route_validates_message_before_delivery() {
    let empty = AgentMessageRoute::Native
        .validate_message(" \n\t ")
        .expect_err("whitespace-only messages should be rejected");
    assert_eq!(
        empty,
        FunctionCallError::RespondToModel("Empty message can't be sent to an agent".to_string())
    );

    let oversized = "x".repeat(MAX_PLAINTEXT_AGENT_MESSAGE_BYTES + 1);
    let error = AgentMessageRoute::ProviderPlaintext
        .validate_message(&oversized)
        .expect_err("oversized plaintext messages should be rejected");
    assert_eq!(
        error,
        FunctionCallError::RespondToModel(format!(
            "Agent messages can't exceed {MAX_PLAINTEXT_AGENT_MESSAGE_BYTES} UTF-8 bytes"
        ))
    );
    assert!(
        AgentMessageRoute::Native
            .validate_message(&oversized)
            .is_ok(),
        "native encrypted messages retain the upstream size behavior"
    );
}

#[test]
fn code_mode_is_restricted_to_plaintext_routes() {
    assert_eq!(
        AgentMessageRoute::Native.messaging_tool_exposure(ToolExposure::Direct),
        ToolExposure::DirectModelOnly
    );
    assert_eq!(
        AgentMessageRoute::ProviderPlaintext.messaging_tool_exposure(ToolExposure::Direct),
        ToolExposure::Direct
    );

    let code_mode_source = ToolCallSource::CodeMode {
        cell_id: "cell".to_string(),
        runtime_tool_call_id: "runtime-call".to_string(),
    };
    let error = communication(
        AgentMessageRoute::Native,
        code_mode_source.clone(),
        /*recipient_is_openai*/ true,
        "plaintext from code mode".to_string(),
    )
    .expect_err("native Code Mode cannot create encrypted collaboration arguments");
    let FunctionCallError::RespondToModel(message) = error else {
        panic!("expected a model-facing validation error");
    };
    assert!(message.contains("direct encrypted tool call"));

    let plaintext = communication(
        AgentMessageRoute::ProviderPlaintext,
        code_mode_source,
        /*recipient_is_openai*/ false,
        "plaintext from code mode".to_string(),
    )
    .expect("plaintext providers can receive Code Mode messages");
    assert!(plaintext.content.contains("plaintext from code mode"));
    assert_eq!(plaintext.encrypted_content, None);
}

#[test]
fn plaintext_message_limit_counts_utf8_bytes() {
    let within_limit = "é".repeat((MAX_PLAINTEXT_AGENT_MESSAGE_BYTES - 512) / 2);
    assert!(
        communication(
            AgentMessageRoute::ProviderPlaintext,
            ToolCallSource::Direct,
            /*recipient_is_openai*/ false,
            within_limit,
        )
        .is_ok(),
        "the configured byte limit should be accepted"
    );

    let over_limit = format!("{}x", "é".repeat(MAX_PLAINTEXT_AGENT_MESSAGE_BYTES / 2));
    let error = communication(
        AgentMessageRoute::ProviderPlaintext,
        ToolCallSource::Direct,
        /*recipient_is_openai*/ false,
        over_limit,
    )
    .expect_err("plaintext messages above the configured byte limit should be rejected");
    let FunctionCallError::RespondToModel(message) = error else {
        panic!("expected a model-facing validation error");
    };
    assert!(message.contains(&format!(
        "can't exceed {MAX_PLAINTEXT_AGENT_MESSAGE_BYTES} UTF-8 bytes"
    )));
}

#[test]
fn provider_plaintext_can_target_openai_or_external_agents() {
    for recipient_is_openai in [true, false] {
        let communication = communication(
            AgentMessageRoute::ProviderPlaintext,
            ToolCallSource::Direct,
            recipient_is_openai,
            "plaintext from an external agent".to_string(),
        )
        .expect("provider plaintext should be accepted");

        assert!(
            communication
                .content
                .contains("plaintext from an external agent")
        );
        assert_eq!(communication.encrypted_content, None);
    }
}

#[test]
fn external_plaintext_rejects_openai_and_accepts_external_agents() {
    let error = communication(
        AgentMessageRoute::ExternalPlaintext,
        ToolCallSource::Direct,
        /*recipient_is_openai*/ true,
        "plaintext for an external agent".to_string(),
    )
    .expect_err("the external route must reject an OpenAI recipient");
    assert_eq!(
        error,
        FunctionCallError::RespondToModel(
            "The selected agent uses the OpenAI model provider; retry with the standard collaboration tool."
                .to_string()
        )
    );

    let communication = communication(
        AgentMessageRoute::ExternalPlaintext,
        ToolCallSource::Direct,
        /*recipient_is_openai*/ false,
        "plaintext for an external agent".to_string(),
    )
    .expect("the external route should accept an external recipient");
    assert!(
        communication
            .content
            .contains("plaintext for an external agent")
    );
    assert_eq!(communication.encrypted_content, None);
}

#[test]
fn native_transport_preserves_encrypted_and_plaintext_paths_without_size_caps() {
    let message = "x".repeat(MAX_PLAINTEXT_AGENT_MESSAGE_BYTES + 1);
    let encrypted = communication(
        AgentMessageRoute::Native,
        ToolCallSource::Direct,
        /*recipient_is_openai*/ true,
        message.clone(),
    )
    .expect("native direct messages should preserve encrypted content");

    assert!(encrypted.content.is_empty());
    assert_eq!(encrypted.encrypted_content, Some(message.clone()));

    let plaintext = communication(
        AgentMessageRoute::Native,
        ToolCallSource::DirectPlaintextMessage,
        /*recipient_is_openai*/ true,
        message,
    )
    .expect("the upstream plaintext fallback should remain supported");

    assert!(plaintext.content.len() > MAX_PLAINTEXT_AGENT_MESSAGE_BYTES);
    assert_eq!(plaintext.encrypted_content, None);
}

#[test]
fn rendered_plaintext_message_includes_the_envelope_in_its_size_limit() {
    let recipient = AgentPath::root()
        .join(&"a".repeat(MAX_PLAINTEXT_AGENT_MESSAGE_BYTES))
        .expect("long lowercase task name should be valid");
    let error = AgentMessageRoute::ExternalPlaintext
        .into_communication(
            &ToolCallSource::Direct,
            /*recipient_is_openai*/ false,
            AgentPath::root(),
            recipient,
            "short payload".to_string(),
            /*trigger_turn*/ true,
        )
        .expect_err("the rendered envelope should count toward the plaintext limit");

    let FunctionCallError::RespondToModel(message) = error else {
        panic!("expected a model-facing validation error");
    };
    assert!(message.contains(&format!(
        "can't exceed {MAX_PLAINTEXT_AGENT_MESSAGE_BYTES} UTF-8 bytes"
    )));
}
