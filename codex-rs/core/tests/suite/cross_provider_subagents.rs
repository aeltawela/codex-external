use super::agent_execution::mount_completed_worker;
use super::agent_execution::mount_encrypted_collaboration_call;
use super::agent_execution::mount_namespaced_collaboration_call;
use super::agent_execution::mount_plaintext_collaboration_call;
use anyhow::Result;
use codex_config::CONFIG_TOML_FILE;
use codex_core::config::AgentRoleConfig;
use codex_core::config::Config;
use codex_features::Feature;
use codex_login::CodexAuth;
use codex_model_provider_info::OPENAI_PROVIDER_ID;
use codex_model_provider_info::WireApi;
use codex_model_provider_info::create_oss_provider_with_base_url;
use codex_models_manager::bundled_models_response;
use codex_protocol::protocol::EventMsg;
use codex_protocol::protocol::MultiAgentVersion;
use core_test_support::responses::ResponseMock;
use core_test_support::responses::ResponsesRequest;
use core_test_support::responses::ev_assistant_message;
use core_test_support::responses::ev_completed;
use core_test_support::responses::ev_function_call_with_namespace;
use core_test_support::responses::ev_response_created;
use core_test_support::responses::mount_sse_once_match;
use core_test_support::responses::sse;
use core_test_support::responses::start_mock_server;
use core_test_support::test_codex::test_codex;
use core_test_support::wait_for_event;
use serde_json::json;
use std::time::Duration;

const CUSTOM_ROLE: &str = "custom_child";
const CUSTOM_PROVIDER: &str = "custom";
const CUSTOM_MODEL: &str = "@preset/external-model";
const CUSTOM_TOKEN: &str = "custom-provider-token";
const SECOND_CUSTOM_ROLE: &str = "second_custom_child";
const SECOND_CUSTOM_PROVIDER: &str = "second-custom";
const SECOND_CUSTOM_MODEL: &str = "@preset/second-external-model";
const SECOND_CUSTOM_TOKEN: &str = "second-custom-provider-token";
const OPENAI_ROLE: &str = "openai_child";
const OPENAI_MODEL: &str = "gpt-5.6-sol";

fn configure_custom_provider(
    config: &mut Config,
    base_url: String,
    multi_agent_version: MultiAgentVersion,
) {
    config.config_layer_stack = config
        .config_layer_stack
        .with_user_config(
            &config.codex_home.join(CONFIG_TOML_FILE),
            toml::from_str(&format!(
                "subagent_model_provider_allowlist = [\"{CUSTOM_PROVIDER}\"]"
            ))
            .expect("valid user config"),
        )
        .expect("user config should be valid");
    config
        .features
        .enable(Feature::Collab)
        .expect("enable collaboration");
    match multi_agent_version {
        MultiAgentVersion::V1 => config
            .features
            .disable(Feature::MultiAgentV2)
            .expect("disable multi-agent v2"),
        MultiAgentVersion::V2 => config
            .features
            .enable(Feature::MultiAgentV2)
            .expect("enable multi-agent v2"),
        MultiAgentVersion::Disabled => panic!("cross-provider test requires multi-agent tools"),
    }
    config
        .features
        .disable(Feature::EnableRequestCompression)
        .expect("disable request compression");
    install_custom_provider_role(
        config,
        CustomProviderRoleSpec {
            base_url: &base_url,
            provider_id: CUSTOM_PROVIDER,
            bearer_token: CUSTOM_TOKEN,
            model: CUSTOM_MODEL,
            role: CUSTOM_ROLE,
            catalog_name: "custom-models.json",
            role_file_name: "custom-child.toml",
            multi_agent_version,
        },
    );
}

struct CustomProviderRoleSpec<'a> {
    base_url: &'a str,
    provider_id: &'a str,
    bearer_token: &'a str,
    model: &'a str,
    role: &'a str,
    catalog_name: &'a str,
    role_file_name: &'a str,
    multi_agent_version: MultiAgentVersion,
}

fn install_custom_provider_role(config: &mut Config, spec: CustomProviderRoleSpec<'_>) {
    let CustomProviderRoleSpec {
        base_url,
        provider_id,
        bearer_token,
        model,
        role,
        catalog_name,
        role_file_name,
        multi_agent_version,
    } = spec;
    let mut provider = create_oss_provider_with_base_url(base_url, WireApi::Responses);
    provider.experimental_bearer_token = Some(bearer_token.into());
    provider.request_max_retries = Some(0);
    provider.stream_max_retries = Some(0);
    config
        .model_providers
        .insert(provider_id.to_string(), provider);

    let mut custom_catalog = bundled_models_response().expect("bundled model catalog should parse");
    custom_catalog
        .models
        .retain(|model| model.slug == "gpt-5.5");
    let custom_model = custom_catalog
        .models
        .first_mut()
        .expect("gpt-5.5 should exist in the bundled model catalog");
    custom_model.slug = model.to_string();
    custom_model.display_name = "Cross-provider test model".to_string();
    custom_model.multi_agent_version = Some(multi_agent_version);
    let catalog_path = config.codex_home.join(catalog_name);
    std::fs::write(
        &catalog_path,
        serde_json::to_vec(&custom_catalog).expect("custom model catalog should serialize"),
    )
    .expect("write custom model catalog");
    let role_path = config.codex_home.join(role_file_name);
    std::fs::write(
        &role_path,
        format!(
            "model = \"{model}\"\nmodel_provider = \"{provider_id}\"\nmodel_catalog_json = \"{catalog_name}\""
        ),
    )
    .expect("write role config");
    config.agent_roles.insert(
        role.to_string(),
        AgentRoleConfig {
            config_file: Some(role_path.to_path_buf()),
            model_provider: Some(provider_id.to_string()),
            model_catalog_json: Some(catalog_path),
            ..Default::default()
        },
    );
}

fn configure_second_custom_provider(
    config: &mut Config,
    base_url: String,
    multi_agent_version: MultiAgentVersion,
) {
    config.config_layer_stack = config
        .config_layer_stack
        .with_user_config(
            &config.codex_home.join(CONFIG_TOML_FILE),
            toml::from_str(&format!(
                "subagent_model_provider_allowlist = [\"{CUSTOM_PROVIDER}\", \"{SECOND_CUSTOM_PROVIDER}\"]"
            ))
            .expect("valid two-provider user config"),
        )
        .expect("two-provider user config should be valid");
    install_custom_provider_role(
        config,
        CustomProviderRoleSpec {
            base_url: &base_url,
            provider_id: SECOND_CUSTOM_PROVIDER,
            bearer_token: SECOND_CUSTOM_TOKEN,
            model: SECOND_CUSTOM_MODEL,
            role: SECOND_CUSTOM_ROLE,
            catalog_name: "second-custom-models.json",
            role_file_name: "second-custom-child.toml",
            multi_agent_version,
        },
    );
}

async fn wait_for_request(mock: &ResponseMock) -> ResponsesRequest {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(request) = mock.last_request() {
                return request;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("timed out waiting for model request")
}

fn request_has_input_type(request: &wiremock::Request, input_type: &str) -> bool {
    serde_json::from_slice::<serde_json::Value>(&request.body)
        .ok()
        .and_then(|body| {
            body.get("input")
                .and_then(serde_json::Value::as_array)
                .cloned()
        })
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.get("type").and_then(serde_json::Value::as_str) == Some(input_type)
            })
        })
}

fn request_has_function_call_output(request: &wiremock::Request, call_id: &str) -> bool {
    serde_json::from_slice::<serde_json::Value>(&request.body)
        .ok()
        .and_then(|body| {
            body.get("input")
                .and_then(serde_json::Value::as_array)
                .cloned()
        })
        .is_some_and(|items| {
            items.iter().any(|item| {
                item.get("type").and_then(serde_json::Value::as_str) == Some("function_call_output")
                    && item.get("call_id").and_then(serde_json::Value::as_str) == Some(call_id)
            })
        })
}

async fn wait_for_agent_message_request(mock: &ResponseMock) -> ResponsesRequest {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if let Some(request) = mock
                .requests()
                .into_iter()
                .find(|request| !request.inputs_of_type("agent_message").is_empty())
            {
                return request;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("timed out waiting for agent-message request")
}

fn assert_encrypted_agent_message(
    request: &ResponsesRequest,
    recipient: &str,
    encrypted_content: &str,
) {
    let messages = request.inputs_of_type("agent_message");
    let matching_messages = messages
        .iter()
        .filter(|message| {
            message["author"] == "/root"
                && message["recipient"] == recipient
                && message["content"].as_array().is_some_and(|content| {
                    content.iter().any(|part| {
                        part["type"] == "encrypted_content"
                            && part["encrypted_content"] == encrypted_content
                    })
                })
        })
        .count();
    assert_eq!(
        matching_messages, 1,
        "the encrypted task should be delivered once"
    );
}

async fn mount_external_spawn_from_agent_message(
    server: &wiremock::MockServer,
    call_id: &'static str,
    arguments: serde_json::Value,
) -> ResponseMock {
    let mut function_call = ev_function_call_with_namespace(
        call_id,
        "external_agents",
        "spawn_agent",
        &arguments.to_string(),
    );
    function_call["item"]["encrypted_function_args"] = json!([]);
    let request_mock = mount_sse_once_match(
        server,
        |request: &wiremock::Request| request_has_input_type(request, "agent_message"),
        sse(vec![
            ev_response_created("resp-v2-delegator"),
            function_call,
            ev_completed("resp-v2-delegator"),
        ]),
    )
    .await;
    mount_sse_once_match(
        server,
        move |request: &wiremock::Request| request_has_function_call_output(request, call_id),
        sse(vec![
            ev_response_created("resp-v2-delegator-complete"),
            ev_assistant_message("msg-v2-delegator-complete", "delegation completed"),
            ev_completed("resp-v2-delegator-complete"),
        ]),
    )
    .await;
    request_mock
}

async fn mount_canonical_spawn_from_user_message(
    server: &wiremock::MockServer,
    prompt: &'static str,
    call_id: &'static str,
    arguments: serde_json::Value,
) -> ResponseMock {
    let mut function_call = ev_function_call_with_namespace(
        call_id,
        "collaboration",
        "spawn_agent",
        &arguments.to_string(),
    );
    function_call["item"]["encrypted_function_args"] = json!([]);
    let request_mock = mount_sse_once_match(
        server,
        move |request: &wiremock::Request| {
            String::from_utf8_lossy(&request.body).contains(prompt)
                && !request_has_input_type(request, "agent_message")
        },
        sse(vec![
            ev_response_created("resp-v2-custom-delegator"),
            function_call,
            ev_completed("resp-v2-custom-delegator"),
        ]),
    )
    .await;
    mount_sse_once_match(
        server,
        move |request: &wiremock::Request| request_has_function_call_output(request, call_id),
        sse(vec![
            ev_response_created("resp-v2-custom-delegator-complete"),
            ev_assistant_message(
                "msg-v2-custom-delegator-complete",
                "custom delegation completed",
            ),
            ev_completed("resp-v2-custom-delegator-complete"),
        ]),
    )
    .await;
    request_mock
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v1_role_switch_routes_child_to_custom_provider() -> Result<()> {
    const ROOT_PROMPT: &str = "spawn one custom-provider worker";
    const CHILD_PROMPT: &str = "check the assigned symbol through v1";
    const CALL_ID: &str = "spawn-v1-custom";

    let root_server = start_mock_server().await;
    let child_server = start_mock_server().await;
    mount_namespaced_collaboration_call(
        &root_server,
        "multi_agent_v1",
        ROOT_PROMPT,
        CALL_ID,
        "spawn_agent",
        json!({
            "message": CHILD_PROMPT,
            "agent_type": CUSTOM_ROLE,
        }),
    )
    .await;
    let child_mock = mount_completed_worker(&child_server, CHILD_PROMPT, CALL_ID).await;
    let child_base_url = format!("{}/v1", child_server.uri());
    let mut builder = test_codex()
        .with_auth(CodexAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(move |config| {
            configure_custom_provider(config, child_base_url, MultiAgentVersion::V1)
        })
        .with_model_info_override("gpt-5.5", |model| {
            model.multi_agent_version = Some(MultiAgentVersion::V1);
        });
    let test = builder.build_with_auto_env(&root_server).await?;

    test.submit_turn(ROOT_PROMPT).await?;

    let child_request = wait_for_request(&child_mock).await;
    assert_eq!(
        child_request.header("authorization").as_deref(),
        Some("Bearer custom-provider-token")
    );
    assert_eq!(child_request.body_json()["model"], CUSTOM_MODEL);
    assert!(child_request.body_contains_text(CHILD_PROMPT));
    assert!(!child_request.body_contains_text(ROOT_PROMPT));
    assert_eq!(test.thread_manager.list_thread_ids().await.len(), 2);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v2_role_switch_routes_child_to_custom_provider_as_user_message() -> Result<()> {
    const ROOT_PROMPT: &str = "spawn one custom-provider V2 worker";
    const CHILD_PROMPT: &str = "check one assigned symbol";
    const CALL_ID: &str = "spawn-v2-custom";

    let root_server = start_mock_server().await;
    let child_server = start_mock_server().await;
    mount_plaintext_collaboration_call(
        &root_server,
        "external_agents",
        ROOT_PROMPT,
        CALL_ID,
        "spawn_agent",
        json!({
            "message": CHILD_PROMPT,
            "task_name": "custom",
            "agent_type": CUSTOM_ROLE,
        }),
    )
    .await;
    let child_mock = mount_completed_worker(&child_server, CHILD_PROMPT, CALL_ID).await;
    let child_base_url = format!("{}/v1", child_server.uri());
    let mut builder = test_codex()
        .with_auth(CodexAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(move |config| {
            configure_custom_provider(config, child_base_url, MultiAgentVersion::V2);
            config.agent_default_subagent_model =
                Some("must-not-override-external-role".to_string());
        })
        .with_model_info_override("gpt-5.5", |model| {
            model.multi_agent_version = Some(MultiAgentVersion::V2);
        });
    let test = builder.build_with_auto_env(&root_server).await?;

    test.submit_turn(ROOT_PROMPT).await?;

    let child_request = wait_for_request(&child_mock).await;
    assert_eq!(
        child_request.header("authorization").as_deref(),
        Some("Bearer custom-provider-token")
    );
    assert_eq!(child_request.body_json()["model"], CUSTOM_MODEL);
    let child_tasks = child_request
        .message_input_texts("user")
        .into_iter()
        .filter(|text| text.contains(CHILD_PROMPT))
        .count();
    assert_eq!(child_tasks, 1, "the child task should be delivered once");
    assert!(!child_request.body_contains_text(ROOT_PROMPT));
    assert!(child_request.inputs_of_type("agent_message").is_empty());
    assert_eq!(test.thread_manager.list_thread_ids().await.len(), 2);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v2_custom_parent_routes_fresh_child_to_openai_provider() -> Result<()> {
    const ROOT_PROMPT: &str = "spawn one OpenAI worker from the custom provider";
    const CHILD_PROMPT: &str = "check the isolated OpenAI task";
    const CALL_ID: &str = "spawn-v2-openai";

    let root_server = start_mock_server().await;
    let child_server = start_mock_server().await;
    let root_mock = mount_canonical_spawn_from_user_message(
        &root_server,
        ROOT_PROMPT,
        CALL_ID,
        json!({
            "message": CHILD_PROMPT,
            "task_name": OPENAI_ROLE,
            "agent_type": OPENAI_ROLE,
            "fork_turns": "none",
        }),
    )
    .await;
    let child_mock = mount_completed_worker(&child_server, CHILD_PROMPT, CALL_ID).await;
    let root_base_url = format!("{}/v1", root_server.uri());
    let mut builder = test_codex()
        .with_auth(CodexAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(move |config| {
            let openai_provider = config.model_provider.clone();
            configure_custom_provider(config, root_base_url, MultiAgentVersion::V2);
            config.config_layer_stack = config
                .config_layer_stack
                .with_user_config(
                    &config.codex_home.join(CONFIG_TOML_FILE),
                    toml::from_str(&format!(
                        "subagent_model_provider_allowlist = [\"{OPENAI_PROVIDER_ID}\"]"
                    ))
                    .expect("valid OpenAI allowlist"),
                )
                .expect("OpenAI allowlist should be valid");
            config
                .model_providers
                .insert(OPENAI_PROVIDER_ID.to_string(), openai_provider);

            let role_path = config.codex_home.join("openai-child.toml");
            std::fs::write(
                &role_path,
                format!("model = \"{OPENAI_MODEL}\"\nmodel_provider = \"{OPENAI_PROVIDER_ID}\""),
            )
            .expect("write OpenAI role");
            config.agent_roles.insert(
                OPENAI_ROLE.to_string(),
                AgentRoleConfig {
                    config_file: Some(role_path.to_path_buf()),
                    model_provider: Some(OPENAI_PROVIDER_ID.to_string()),
                    ..Default::default()
                },
            );

            config.model_provider_id = CUSTOM_PROVIDER.to_string();
            config.model_provider = config.model_providers[CUSTOM_PROVIDER].clone();
            config.model = Some("gpt-5.5".to_string());
        })
        .with_model_info_override("gpt-5.5", |model| {
            model.multi_agent_version = Some(MultiAgentVersion::V2);
        });
    let test = builder.build_with_auto_env(&child_server).await?;

    test.submit_turn(ROOT_PROMPT).await?;

    let root_request = wait_for_request(&root_mock).await;
    assert_eq!(
        root_request.header("authorization").as_deref(),
        Some("Bearer custom-provider-token")
    );
    assert_eq!(root_request.body_json()["model"], "gpt-5.5");
    assert!(root_request.inputs_of_type("agent_message").is_empty());

    let child_request = wait_for_request(&child_mock).await;
    assert_eq!(
        child_request.header("authorization").as_deref(),
        Some("Bearer Access Token")
    );
    assert_eq!(
        child_request.header("chatgpt-account-id").as_deref(),
        Some("account_id")
    );
    assert_eq!(child_request.body_json()["model"], OPENAI_MODEL);
    let child_messages = child_request.inputs_of_type("agent_message");
    assert_eq!(
        child_messages.len(),
        1,
        "the fresh child task should be delivered once"
    );
    let child_message = &child_messages[0];
    assert_eq!(child_message["author"], "/root");
    assert_eq!(child_message["recipient"], format!("/root/{OPENAI_ROLE}"));
    assert!(
        child_message["content"]
            .as_array()
            .is_some_and(|content| content.iter().any(|part| {
                part["type"] == "input_text"
                    && part["text"]
                        .as_str()
                        .is_some_and(|text| text.contains(CHILD_PROMPT))
            }))
    );
    assert!(child_message["content"].as_array().is_some_and(|content| {
        content
            .iter()
            .all(|part| part["type"] != "encrypted_content")
    }));
    assert!(!child_request.body_contains_text(ROOT_PROMPT));
    assert_eq!(test.thread_manager.list_thread_ids().await.len(), 2);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v2_followup_to_custom_provider_rejects_encrypted_then_delivers_plaintext_user_message()
-> Result<()> {
    const ROOT_PROMPT: &str = "spawn a custom-provider worker for follow-up";
    const CHILD_PROMPT: &str = "complete the initial external task";
    const ENCRYPTED_FOLLOWUP_PROMPT: &str = "send an encrypted external follow-up";
    const ENCRYPTED_FOLLOWUP: &str = "opaque-encrypted-external-follow-up";
    const EVICT_PROMPT: &str = "spawn a same-provider replacement";
    const EVICT_CHILD_PROMPT: &str = "opaque-encrypted-replacement-task";
    const PLAINTEXT_FOLLOWUP_PROMPT: &str = "send a plaintext external follow-up";
    const PLAINTEXT_FOLLOWUP: &str = "complete the external follow-up task";

    let root_server = start_mock_server().await;
    let child_server = start_mock_server().await;
    mount_plaintext_collaboration_call(
        &root_server,
        "external_agents",
        ROOT_PROMPT,
        "spawn-v2-followup-custom",
        "spawn_agent",
        json!({
            "message": CHILD_PROMPT,
            "task_name": "custom",
            "agent_type": CUSTOM_ROLE,
        }),
    )
    .await;
    let initial_child_mock =
        mount_completed_worker(&child_server, CHILD_PROMPT, "spawn-v2-followup-custom").await;
    mount_encrypted_collaboration_call(
        &root_server,
        "collaboration",
        ENCRYPTED_FOLLOWUP_PROMPT,
        "encrypted-v2-followup-custom",
        "followup_task",
        json!({
            "target": "custom",
            "message": ENCRYPTED_FOLLOWUP,
        }),
    )
    .await;
    mount_plaintext_collaboration_call(
        &root_server,
        "external_agents",
        PLAINTEXT_FOLLOWUP_PROMPT,
        "plaintext-v2-followup-custom",
        "followup_task",
        json!({
            "target": "custom",
            "message": PLAINTEXT_FOLLOWUP,
        }),
    )
    .await;
    mount_encrypted_collaboration_call(
        &root_server,
        "collaboration",
        EVICT_PROMPT,
        "spawn-v2-replacement",
        "spawn_agent",
        json!({
            "message": EVICT_CHILD_PROMPT,
            "task_name": "replacement",
            "fork_turns": "none",
        }),
    )
    .await;
    let replacement_mock =
        mount_completed_worker(&root_server, EVICT_CHILD_PROMPT, "spawn-v2-replacement").await;
    let plaintext_child_mock = mount_completed_worker(
        &child_server,
        PLAINTEXT_FOLLOWUP,
        "plaintext-v2-followup-custom",
    )
    .await;

    let child_base_url = format!("{}/v1", child_server.uri());
    let mut builder = test_codex()
        .with_auth(CodexAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(move |config| {
            configure_custom_provider(config, child_base_url, MultiAgentVersion::V2);
            config.multi_agent_v2.max_concurrent_threads_per_session = 2;
        })
        .with_model_info_override("gpt-5.5", |model| {
            model.multi_agent_version = Some(MultiAgentVersion::V2);
        });
    let test = builder.build_with_auto_env(&root_server).await?;
    let mut created_threads = test.thread_manager.subscribe_thread_created();

    test.submit_turn(ROOT_PROMPT).await?;
    let child_thread_id = tokio::time::timeout(Duration::from_secs(10), created_threads.recv())
        .await
        .expect("timed out waiting for the external child thread")?;
    let child_thread = test.thread_manager.get_thread(child_thread_id).await?;
    wait_for_event(child_thread.as_ref(), |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
    assert_eq!(initial_child_mock.requests().len(), 1);

    test.submit_turn(ENCRYPTED_FOLLOWUP_PROMPT).await?;
    assert!(
        root_server
            .received_requests()
            .await
            .unwrap_or_default()
            .iter()
            .any(|request| String::from_utf8_lossy(&request.body)
                .contains("retry with the corresponding `external_agents` collaboration tool")),
        "the encrypted follow-up should return the provider-specific retry error"
    );
    drop(child_thread);
    test.submit_turn(EVICT_PROMPT).await?;
    let replacement_thread_id =
        tokio::time::timeout(Duration::from_secs(10), created_threads.recv())
            .await
            .expect("timed out waiting for the replacement thread")?;
    let replacement_thread = test
        .thread_manager
        .get_thread(replacement_thread_id)
        .await?;
    wait_for_event(replacement_thread.as_ref(), |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;
    assert_eq!(replacement_mock.requests().len(), 1);
    assert!(
        test.thread_manager
            .get_thread(child_thread_id)
            .await
            .is_err(),
        "the external child should be evicted before its follow-up"
    );

    test.submit_turn(PLAINTEXT_FOLLOWUP_PROMPT).await?;
    let reloaded_child = test.thread_manager.get_thread(child_thread_id).await?;
    wait_for_event(reloaded_child.as_ref(), |event| {
        matches!(event, EventMsg::TurnComplete(_))
    })
    .await;

    let followup_request = plaintext_child_mock.single_request();
    assert_eq!(
        followup_request.header("authorization").as_deref(),
        Some("Bearer custom-provider-token")
    );
    assert_eq!(followup_request.body_json()["model"], CUSTOM_MODEL);
    assert_eq!(
        followup_request
            .message_input_texts("user")
            .into_iter()
            .filter(|text| text.contains(PLAINTEXT_FOLLOWUP))
            .count(),
        1,
        "the plaintext follow-up should be delivered once as a user message"
    );
    assert_eq!(
        followup_request
            .message_input_texts("user")
            .into_iter()
            .filter(|text| text.contains(CHILD_PROMPT))
            .count(),
        1,
        "the initial child task should appear exactly once in the child history"
    );
    assert!(followup_request.inputs_of_type("agent_message").is_empty());
    assert!(!followup_request.body_contains_text(ENCRYPTED_FOLLOWUP));
    assert!(!followup_request.body_contains_text(ROOT_PROMPT));
    assert!(!followup_request.body_contains_text(ENCRYPTED_FOLLOWUP_PROMPT));
    assert!(!followup_request.body_contains_text(PLAINTEXT_FOLLOWUP_PROMPT));
    let child_response_requests = child_server
        .received_requests()
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|request| request.method == "POST" && request.url.path().ends_with("/responses"))
        .collect::<Vec<_>>();
    assert_eq!(
        child_response_requests.len(),
        2,
        "only the initial task and plaintext follow-up may reach the external provider"
    );
    assert!(
        child_response_requests
            .iter()
            .all(|request| !String::from_utf8_lossy(&request.body).contains(ENCRYPTED_FOLLOWUP)),
        "the encrypted follow-up must never reach the external provider"
    );
    assert_eq!(test.thread_manager.list_thread_ids().await.len(), 2);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v2_nested_role_switch_routes_fresh_grandchild_to_custom_provider() -> Result<()> {
    const ROOT_PROMPT: &str = "spawn a same-provider delegator";
    const DELEGATOR_CIPHERTEXT: &str = "opaque-encrypted-delegator-task";
    const CHILD_PROMPT: &str = "check the assigned symbol";
    const ROOT_CALL_ID: &str = "spawn-v2-delegator";
    const DELEGATOR_CALL_ID: &str = "spawn-v2-custom";

    let root_server = start_mock_server().await;
    let child_server = start_mock_server().await;
    mount_encrypted_collaboration_call(
        &root_server,
        "collaboration",
        ROOT_PROMPT,
        ROOT_CALL_ID,
        "spawn_agent",
        json!({
            "message": DELEGATOR_CIPHERTEXT,
            "task_name": "delegator",
            "fork_turns": "none",
        }),
    )
    .await;
    let delegator_mock = mount_external_spawn_from_agent_message(
        &root_server,
        DELEGATOR_CALL_ID,
        json!({
            "message": CHILD_PROMPT,
            "task_name": "custom",
            "agent_type": CUSTOM_ROLE,
        }),
    )
    .await;
    let child_mock = mount_completed_worker(&child_server, CHILD_PROMPT, DELEGATOR_CALL_ID).await;
    let child_base_url = format!("{}/v1", child_server.uri());
    let mut builder = test_codex()
        .with_auth(CodexAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(move |config| {
            configure_custom_provider(config, child_base_url, MultiAgentVersion::V2)
        })
        .with_model_info_override("gpt-5.5", |model| {
            model.multi_agent_version = Some(MultiAgentVersion::V2);
        });
    let test = builder.build_with_auto_env(&root_server).await?;

    test.submit_turn(ROOT_PROMPT).await?;

    let delegator_request = wait_for_agent_message_request(&delegator_mock).await;
    assert_encrypted_agent_message(&delegator_request, "/root/delegator", DELEGATOR_CIPHERTEXT);
    assert!(!delegator_request.body_contains_text(ROOT_PROMPT));
    let child_request = wait_for_request(&child_mock).await;
    assert_eq!(
        child_request.header("authorization").as_deref(),
        Some("Bearer custom-provider-token")
    );
    assert_eq!(child_request.body_json()["model"], CUSTOM_MODEL);
    let child_tasks = child_request
        .message_input_texts("user")
        .into_iter()
        .filter(|text| text.contains(CHILD_PROMPT))
        .count();
    assert_eq!(
        child_tasks, 1,
        "the delegated task should be delivered once"
    );
    assert_eq!(
        child_request.inputs_of_type("agent_message"),
        Vec::<serde_json::Value>::new()
    );
    assert!(!child_request.body_contains_text(ROOT_PROMPT));
    assert!(!child_request.body_contains_text(DELEGATOR_CIPHERTEXT));
    assert_eq!(test.thread_manager.list_thread_ids().await.len(), 3);
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn v2_custom_child_routes_fresh_grandchild_to_second_custom_provider() -> Result<()> {
    const ROOT_PROMPT: &str = "spawn the first external delegator";
    const DELEGATOR_PROMPT: &str = "delegate one fresh task to the second provider";
    const GRANDCHILD_PROMPT: &str = "inspect the second provider target";
    const ROOT_CALL_ID: &str = "spawn-v2-first-custom";
    const DELEGATOR_CALL_ID: &str = "spawn-v2-second-custom";

    let root_server = start_mock_server().await;
    let first_custom_server = start_mock_server().await;
    let second_custom_server = start_mock_server().await;
    mount_plaintext_collaboration_call(
        &root_server,
        "external_agents",
        ROOT_PROMPT,
        ROOT_CALL_ID,
        "spawn_agent",
        json!({
            "message": DELEGATOR_PROMPT,
            "task_name": "first_custom",
            "agent_type": CUSTOM_ROLE,
            "fork_turns": "none",
        }),
    )
    .await;
    let delegator_mock = mount_canonical_spawn_from_user_message(
        &first_custom_server,
        DELEGATOR_PROMPT,
        DELEGATOR_CALL_ID,
        json!({
            "message": GRANDCHILD_PROMPT,
            "task_name": "second_custom",
            "agent_type": SECOND_CUSTOM_ROLE,
            "fork_turns": "none",
        }),
    )
    .await;
    let grandchild_mock =
        mount_completed_worker(&second_custom_server, GRANDCHILD_PROMPT, DELEGATOR_CALL_ID).await;
    let first_custom_base_url = format!("{}/v1", first_custom_server.uri());
    let second_custom_base_url = format!("{}/v1", second_custom_server.uri());
    let mut builder = test_codex()
        .with_auth(CodexAuth::create_dummy_chatgpt_auth_for_testing())
        .with_config(move |config| {
            configure_custom_provider(config, first_custom_base_url, MultiAgentVersion::V2);
            configure_second_custom_provider(config, second_custom_base_url, MultiAgentVersion::V2);
        })
        .with_model_info_override("gpt-5.5", |model| {
            model.multi_agent_version = Some(MultiAgentVersion::V2);
        });
    let test = builder.build_with_auto_env(&root_server).await?;

    test.submit_turn(ROOT_PROMPT).await?;

    let delegator_request = wait_for_request(&delegator_mock).await;
    let first_authorization = format!("Bearer {CUSTOM_TOKEN}");
    assert_eq!(
        delegator_request.header("authorization").as_deref(),
        Some(first_authorization.as_str())
    );
    assert_eq!(delegator_request.body_json()["model"], CUSTOM_MODEL);
    assert_eq!(
        delegator_request
            .message_input_texts("user")
            .into_iter()
            .filter(|text| text.contains(DELEGATOR_PROMPT))
            .count(),
        1,
        "the first custom task should be delivered once"
    );
    assert!(delegator_request.inputs_of_type("agent_message").is_empty());
    assert!(!delegator_request.body_contains_text(ROOT_PROMPT));

    let grandchild_request = wait_for_request(&grandchild_mock).await;
    let second_authorization = format!("Bearer {SECOND_CUSTOM_TOKEN}");
    assert_eq!(
        grandchild_request.header("authorization").as_deref(),
        Some(second_authorization.as_str())
    );
    assert_eq!(grandchild_request.body_json()["model"], SECOND_CUSTOM_MODEL);
    assert_eq!(
        grandchild_request
            .message_input_texts("user")
            .into_iter()
            .filter(|text| text.contains(GRANDCHILD_PROMPT))
            .count(),
        1,
        "the second custom task should be delivered once"
    );
    assert!(
        grandchild_request
            .inputs_of_type("agent_message")
            .is_empty()
    );
    assert!(!grandchild_request.body_contains_text(ROOT_PROMPT));
    assert!(!grandchild_request.body_contains_text(DELEGATOR_PROMPT));

    let mut thread_routes = Vec::new();
    for thread_id in test.thread_manager.list_thread_ids().await {
        let snapshot = test
            .thread_manager
            .get_thread(thread_id)
            .await?
            .config_snapshot()
            .await;
        thread_routes.push((snapshot.model_provider_id, snapshot.model));
    }
    assert!(
        thread_routes
            .iter()
            .any(|(provider, model)| { provider == CUSTOM_PROVIDER && model == CUSTOM_MODEL })
    );
    assert!(thread_routes.iter().any(|(provider, model)| {
        provider == SECOND_CUSTOM_PROVIDER && model == SECOND_CUSTOM_MODEL
    }));
    assert_eq!(thread_routes.len(), 3);
    Ok(())
}
