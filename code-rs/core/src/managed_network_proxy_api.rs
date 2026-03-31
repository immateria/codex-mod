use std::collections::HashMap;
use std::net::SocketAddr;

#[cfg(feature = "managed-network-proxy")]
#[derive(Clone, Debug)]
pub struct ManagedNetworkProxy(code_network_proxy::NetworkProxy);

#[cfg(not(feature = "managed-network-proxy"))]
#[derive(Clone, Debug, Default)]
pub struct ManagedNetworkProxy {
    _private: (),
}

impl ManagedNetworkProxy {
    #[cfg(feature = "managed-network-proxy")]
    pub(crate) fn http_addr(&self) -> SocketAddr {
        self.0.http_addr()
    }

    #[cfg(not(feature = "managed-network-proxy"))]
    pub(crate) fn http_addr(&self) -> SocketAddr {
        SocketAddr::from(([127, 0, 0, 1], 0))
    }

    #[cfg(feature = "managed-network-proxy")]
    pub(crate) fn apply_to_env(&self, env: &mut HashMap<String, String>) {
        self.0.apply_to_env(env);
    }

    #[cfg(not(feature = "managed-network-proxy"))]
    pub(crate) fn apply_to_env(&self, _env: &mut HashMap<String, String>) {}

    #[cfg(feature = "managed-network-proxy")]
    pub(crate) fn apply_to_env_for_attempt(
        &self,
        env: &mut HashMap<String, String>,
        network_attempt_id: Option<&str>,
    ) {
        self.0.apply_to_env_for_attempt(env, network_attempt_id);
    }

    #[cfg(not(feature = "managed-network-proxy"))]
    pub(crate) fn apply_to_env_for_attempt(
        &self,
        _env: &mut HashMap<String, String>,
        _network_attempt_id: Option<&str>,
    ) {
    }
}

#[cfg(feature = "managed-network-proxy")]
impl From<code_network_proxy::NetworkProxy> for ManagedNetworkProxy {
    fn from(proxy: code_network_proxy::NetworkProxy) -> Self {
        Self(proxy)
    }
}

pub(crate) const NETWORK_ATTEMPT_USERNAME_PREFIX: &str = "codex-net-attempt-";

pub(crate) fn proxy_username_for_attempt_id(attempt_id: &str) -> String {
    format!("{NETWORK_ATTEMPT_USERNAME_PREFIX}{attempt_id}")
}

pub(crate) const PROXY_URL_ENV_KEYS: &[&str] = &[
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "FTP_PROXY",
    "YARN_HTTP_PROXY",
    "YARN_HTTPS_PROXY",
    "NPM_CONFIG_HTTP_PROXY",
    "NPM_CONFIG_HTTPS_PROXY",
    "NPM_CONFIG_PROXY",
    "BUNDLE_HTTP_PROXY",
    "BUNDLE_HTTPS_PROXY",
    "PIP_PROXY",
    "DOCKER_HTTP_PROXY",
    "DOCKER_HTTPS_PROXY",
];

pub(crate) const ALLOW_LOCAL_BINDING_ENV_KEY: &str = "CODEX_NETWORK_ALLOW_LOCAL_BINDING";

pub(crate) fn proxy_url_env_value<'a>(
    env: &'a HashMap<String, String>,
    canonical_key: &str,
) -> Option<&'a str> {
    if let Some(value) = env.get(canonical_key) {
        return Some(value.as_str());
    }
    let lower_key = canonical_key.to_ascii_lowercase();
    env.get(lower_key.as_str()).map(String::as_str)
}

pub(crate) fn has_proxy_url_env_vars(env: &HashMap<String, String>) -> bool {
    PROXY_URL_ENV_KEYS
        .iter()
        .any(|key| proxy_url_env_value(env, key).is_some_and(|value| !value.trim().is_empty()))
}
