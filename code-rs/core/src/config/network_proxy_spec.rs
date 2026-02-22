use async_trait::async_trait;
use code_network_proxy::BlockedRequestObserver;
use code_network_proxy::ConfigReloader;
use code_network_proxy::ConfigState;
use code_network_proxy::NetworkDecision;
use code_network_proxy::NetworkPolicyDecider;
use code_network_proxy::NetworkProxy;
use code_network_proxy::NetworkProxyConfig;
use code_network_proxy::NetworkProxyConstraints;
use code_network_proxy::NetworkProxyHandle;
use code_network_proxy::NetworkProxyState;
use code_network_proxy::build_config_state;
use code_network_proxy::validate_policy_against_constraints;
use std::sync::Arc;

use crate::protocol::SandboxPolicy;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkProxySpec {
    config: NetworkProxyConfig,
    constraints: NetworkProxyConstraints,
}

pub struct StartedNetworkProxy {
    proxy: NetworkProxy,
    handle: NetworkProxyHandle,
}

impl StartedNetworkProxy {
    fn new(proxy: NetworkProxy, handle: NetworkProxyHandle) -> Self {
        Self { proxy, handle }
    }

    pub fn proxy(&self) -> NetworkProxy {
        // Ensure the handle stays "used" so we keep the listeners alive.
        let _ = &self.handle;
        self.proxy.clone()
    }
}

#[derive(Clone)]
struct StaticNetworkProxyReloader {
    state: ConfigState,
}

impl StaticNetworkProxyReloader {
    fn new(state: ConfigState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ConfigReloader for StaticNetworkProxyReloader {
    async fn maybe_reload(&self) -> anyhow::Result<Option<ConfigState>> {
        Ok(None)
    }

    async fn reload_now(&self) -> anyhow::Result<ConfigState> {
        Ok(self.state.clone())
    }

    fn source_label(&self) -> String {
        "StaticNetworkProxyReloader".to_string()
    }
}

impl NetworkProxySpec {
    pub fn from_config(config: NetworkProxyConfig) -> std::io::Result<Self> {
        let constraints = NetworkProxyConstraints::default();
        validate_policy_against_constraints(&config, &constraints).map_err(|err| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("invalid network proxy config: {err}"),
            )
        })?;
        Ok(Self { config, constraints })
    }

    pub async fn start_proxy(
        &self,
        sandbox_policy: &SandboxPolicy,
        policy_decider: Option<Arc<dyn NetworkPolicyDecider>>,
        blocked_request_observer: Option<Arc<dyn BlockedRequestObserver>>,
        enable_network_approval_flow: bool,
    ) -> std::io::Result<StartedNetworkProxy> {
        let state = build_config_state(self.config.clone(), self.constraints.clone()).map_err(|err| {
            std::io::Error::other(format!("failed to build network proxy state: {err}"))
        })?;
        let reloader = Arc::new(StaticNetworkProxyReloader::new(state.clone()));
        let state = NetworkProxyState::with_reloader(state, reloader);

        let mut builder = NetworkProxy::builder().state(Arc::new(state));

        if enable_network_approval_flow
            && matches!(
                sandbox_policy,
                SandboxPolicy::ReadOnly | SandboxPolicy::WorkspaceWrite { .. }
            )
        {
            builder = match policy_decider {
                Some(policy_decider) => builder.policy_decider_arc(policy_decider),
                None => builder.policy_decider(|_request| async {
                    // In restricted sandbox modes, allowlist misses should ask for
                    // explicit network approval instead of hard-denying.
                    NetworkDecision::ask("not_allowed")
                }),
            };
        }

        if let Some(blocked_request_observer) = blocked_request_observer {
            builder = builder.blocked_request_observer_arc(blocked_request_observer);
        }

        let proxy = builder.build().await.map_err(|err| {
            std::io::Error::other(format!("failed to build network proxy: {err}"))
        })?;
        let handle = proxy.run().await.map_err(|err| {
            std::io::Error::other(format!("failed to run network proxy: {err}"))
        })?;
        Ok(StartedNetworkProxy::new(proxy, handle))
    }
}
