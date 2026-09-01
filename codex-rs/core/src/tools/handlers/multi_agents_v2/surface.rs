use super::followup_task::Handler as FollowupTaskHandler;
use super::interrupt_agent::Handler as InterruptAgentHandler;
use super::list_agents::Handler as ListAgentsHandler;
use super::send_message::Handler as SendMessageHandler;
use super::spawn::Handler as SpawnAgentHandler;
use super::wait::Handler as WaitAgentHandler;
use crate::session::session::Session;
use crate::tools::context::ToolInvocation;
use crate::tools::handlers::multi_agent_message::AgentMessageRoute;
use crate::tools::handlers::multi_agents_spec::SpawnAgentToolOptions;
use crate::tools::handlers::multi_agents_spec::WaitAgentTimeoutOptions;
use crate::tools::registry::CoreToolRuntime;
use crate::tools::registry::ToolRegistry;
use codex_tools::ResponsesApiNamespace;
use codex_tools::ResponsesApiNamespaceTool;
use codex_tools::ToolExecutor;
use codex_tools::ToolExposure;
use codex_tools::ToolName;
use codex_tools::ToolSearchInfo;
use codex_tools::ToolSpec;
use futures::future::BoxFuture;
use std::sync::Arc;

const CANONICAL_NAMESPACE_DESCRIPTION: &str = "Tools for spawning and managing sub-agents.";
pub(crate) const EXTERNAL_AGENT_NAMESPACE: &str = "external_agents";
const EXTERNAL_NAMESPACE_DESCRIPTION: &str =
    "Plaintext collaboration tools for authorized agents using non-OpenAI model providers.";

pub(crate) struct MultiAgentV2ToolOptions<'a> {
    pub(crate) namespace: Option<&'a str>,
    pub(crate) exposure: ToolExposure,
    pub(crate) provider_is_openai: bool,
    pub(crate) external_bridge_enabled: bool,
    pub(crate) spawn: SpawnAgentToolOptions,
    pub(crate) wait_agent: Option<WaitAgentTimeoutOptions>,
}

pub(crate) fn register_multi_agent_v2_tools(
    registry: &mut ToolRegistry,
    options: MultiAgentV2ToolOptions<'_>,
) {
    let MultiAgentV2ToolOptions {
        namespace,
        exposure,
        provider_is_openai,
        external_bridge_enabled,
        spawn,
        wait_agent,
    } = options;
    let external_spawn =
        (provider_is_openai && external_bridge_enabled).then(|| SpawnAgentToolOptions {
            hide_agent_type_model_reasoning: spawn.hide_agent_type_model_reasoning,
            multi_agent_version: spawn.multi_agent_version,
            ..Default::default()
        });
    MessagingSurface::canonical(namespace, exposure, provider_is_openai).register(registry, spawn);

    if let Some(external_spawn) = external_spawn {
        MessagingSurface::external().register(registry, external_spawn);
    }

    if let Some(wait_agent) = wait_agent {
        registry.register_trusted_with_exposure(
            namespaced_handler(WaitAgentHandler::new(wait_agent), namespace),
            exposure,
        );
    }
    registry.register_trusted_with_exposure(
        namespaced_handler(InterruptAgentHandler, namespace),
        exposure,
    );
    registry
        .register_trusted_with_exposure(namespaced_handler(ListAgentsHandler, namespace), exposure);
}

struct MessagingSurface {
    namespace: Option<String>,
    namespace_description: &'static str,
    exposure: ToolExposure,
    message_route: AgentMessageRoute,
}

impl MessagingSurface {
    fn canonical(
        namespace: Option<&str>,
        exposure: ToolExposure,
        provider_is_openai: bool,
    ) -> Self {
        let message_route = if provider_is_openai {
            AgentMessageRoute::Native
        } else {
            AgentMessageRoute::ProviderPlaintext
        };
        Self {
            namespace: namespace.map(str::to_string),
            namespace_description: CANONICAL_NAMESPACE_DESCRIPTION,
            exposure: message_route.messaging_tool_exposure(exposure),
            message_route,
        }
    }

    fn external() -> Self {
        Self {
            namespace: Some(EXTERNAL_AGENT_NAMESPACE.to_string()),
            namespace_description: EXTERNAL_NAMESPACE_DESCRIPTION,
            exposure: ToolExposure::DirectModelOnly,
            message_route: AgentMessageRoute::ExternalPlaintext,
        }
    }

    fn register(self, registry: &mut ToolRegistry, spawn_options: SpawnAgentToolOptions) {
        let Self {
            namespace,
            namespace_description,
            exposure,
            message_route,
        } = self;
        let namespace = namespace.as_deref();
        registry.register_trusted_with_exposure(
            namespaced_handler_with_description(
                SpawnAgentHandler::new(spawn_options, message_route),
                namespace,
                namespace_description,
            ),
            exposure,
        );
        registry.register_trusted_with_exposure(
            namespaced_handler_with_description(
                SendMessageHandler::new(message_route),
                namespace,
                namespace_description,
            ),
            exposure,
        );
        registry.register_trusted_with_exposure(
            namespaced_handler_with_description(
                FollowupTaskHandler::new(message_route),
                namespace,
                namespace_description,
            ),
            exposure,
        );
    }
}

fn namespaced_handler(
    handler: impl CoreToolRuntime + 'static,
    namespace: Option<&str>,
) -> Arc<dyn CoreToolRuntime> {
    namespaced_handler_with_description(handler, namespace, CANONICAL_NAMESPACE_DESCRIPTION)
}

fn namespaced_handler_with_description(
    handler: impl CoreToolRuntime + 'static,
    namespace: Option<&str>,
    namespace_description: &str,
) -> Arc<dyn CoreToolRuntime> {
    match namespace {
        Some(namespace) => Arc::new(NamespaceOverride {
            handler: Arc::new(handler),
            namespace: namespace.to_string(),
            namespace_description: namespace_description.to_string(),
        }),
        None => Arc::new(handler),
    }
}

struct NamespaceOverride {
    handler: Arc<dyn CoreToolRuntime>,
    namespace: String,
    namespace_description: String,
}

impl ToolExecutor<ToolInvocation> for NamespaceOverride {
    fn tool_name(&self) -> ToolName {
        ToolName::namespaced(self.namespace.clone(), self.handler.tool_name().name)
    }

    fn spec(&self) -> ToolSpec {
        match self.handler.spec() {
            ToolSpec::Function(tool) => ToolSpec::Namespace(ResponsesApiNamespace {
                name: self.namespace.clone(),
                description: self.namespace_description.clone(),
                tools: vec![ResponsesApiNamespaceTool::Function(tool)],
            }),
            spec => spec,
        }
    }

    fn exposure(&self) -> ToolExposure {
        self.handler.exposure()
    }

    fn supports_parallel_tool_calls(&self) -> bool {
        self.handler.supports_parallel_tool_calls()
    }

    fn search_info(&self) -> Option<ToolSearchInfo> {
        self.handler.search_info()
    }

    fn handle<'a>(&'a self, invocation: ToolInvocation) -> codex_tools::ToolExecutorFuture<'a>
    where
        ToolInvocation: 'a,
    {
        self.handler.handle(invocation)
    }
}

impl CoreToolRuntime for NamespaceOverride {
    fn direct_tool_call_source_policy(&self) -> crate::tools::context::DirectToolCallSourcePolicy {
        self.handler.direct_tool_call_source_policy()
    }

    fn wait_until_ready<'a>(&'a self, session: &'a Arc<Session>) -> Option<BoxFuture<'a, ()>> {
        self.handler.wait_until_ready(session)
    }

    fn matches_kind(&self, payload: &crate::tools::context::ToolPayload) -> bool {
        self.handler.matches_kind(payload)
    }

    fn create_diff_consumer(
        &self,
    ) -> Option<Box<dyn crate::tools::registry::ToolArgumentDiffConsumer>> {
        self.handler.create_diff_consumer()
    }
}
