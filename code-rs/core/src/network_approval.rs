use code_network_proxy::NetworkDecision;
use code_network_proxy::NetworkPolicyDecider;
use code_network_proxy::NetworkPolicyRequest;
use code_network_proxy::NetworkProtocol;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::codex::Session;
use crate::protocol::NetworkApprovalContext;
use crate::protocol::NetworkApprovalProtocol;
use crate::protocol::ReviewDecision;

const REASON_NOT_ALLOWED: &str = "not_allowed";

struct NetworkApprovalAttempt {
    turn_id: String,
    call_id: String,
    command: Vec<String>,
    cwd: PathBuf,
    approved_hosts: Mutex<HashSet<String>>,
    denied_by_user: Mutex<bool>,
}

#[derive(Default)]
pub(crate) struct NetworkApprovalService {
    attempts: Mutex<HashMap<String, Arc<NetworkApprovalAttempt>>>,
    session_approved_hosts: Mutex<HashSet<String>>,
}

pub(crate) struct NetworkAttemptGuard {
    attempt_id: Option<String>,
    network_approval: Arc<NetworkApprovalService>,
}

impl std::fmt::Debug for NetworkAttemptGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NetworkAttemptGuard")
            .field("attempt_id", &self.attempt_id)
            .finish()
    }
}

impl NetworkAttemptGuard {
    pub(crate) fn new(network_approval: Arc<NetworkApprovalService>, attempt_id: String) -> Self {
        Self {
            attempt_id: Some(attempt_id),
            network_approval,
        }
    }
}

impl Drop for NetworkAttemptGuard {
    fn drop(&mut self) {
        let Some(attempt_id) = self.attempt_id.take() else {
            return;
        };
        let network_approval = Arc::clone(&self.network_approval);
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                network_approval.unregister_attempt(&attempt_id).await;
            });
        }
    }
}

impl NetworkApprovalService {
    pub(crate) async fn register_attempt(
        &self,
        attempt_id: String,
        turn_id: String,
        call_id: String,
        command: Vec<String>,
        cwd: PathBuf,
    ) {
        let mut attempts = self.attempts.lock().await;
        attempts.insert(
            attempt_id,
            Arc::new(NetworkApprovalAttempt {
                turn_id,
                call_id,
                command,
                cwd,
                approved_hosts: Mutex::new(HashSet::new()),
                denied_by_user: Mutex::new(false),
            }),
        );
    }

    pub(crate) async fn unregister_attempt(&self, attempt_id: &str) {
        let mut attempts = self.attempts.lock().await;
        attempts.remove(attempt_id);
    }

    async fn resolve_attempt_for_request(
        &self,
        request: &NetworkPolicyRequest,
    ) -> Option<Arc<NetworkApprovalAttempt>> {
        let attempts = self.attempts.lock().await;

        if let Some(attempt_id) = request.attempt_id.as_deref() {
            return attempts.get(attempt_id).cloned();
        }

        if attempts.len() == 1 {
            return attempts.values().next().cloned();
        }

        None
    }

    pub(crate) async fn handle_policy_request(
        &self,
        session: &Session,
        request: NetworkPolicyRequest,
    ) -> NetworkDecision {
        let host_display = request.host.clone();
        let host = request.host.to_ascii_lowercase();

        {
            let approved_hosts = self.session_approved_hosts.lock().await;
            if approved_hosts.contains(host.as_str()) {
                return NetworkDecision::Allow;
            }
        }

        let Some(attempt) = self.resolve_attempt_for_request(&request).await else {
            return NetworkDecision::deny(REASON_NOT_ALLOWED);
        };

        {
            let denied = attempt.denied_by_user.lock().await;
            if *denied {
                return NetworkDecision::deny(REASON_NOT_ALLOWED);
            }
        }

        {
            let approved_hosts = attempt.approved_hosts.lock().await;
            if approved_hosts.contains(host.as_str()) {
                return NetworkDecision::Allow;
            }
        }

        let protocol = match request.protocol {
            NetworkProtocol::Http => NetworkApprovalProtocol::Http,
            NetworkProtocol::HttpsConnect => NetworkApprovalProtocol::Https,
            NetworkProtocol::Socks5Tcp => NetworkApprovalProtocol::Socks5Tcp,
            NetworkProtocol::Socks5Udp => NetworkApprovalProtocol::Socks5Udp,
        };

        // Ensure each network approval prompt has a unique approval_id; the TUI routes
        // approval responses by the effective approval id (approval_id or call_id fallback).
        let approval_id = format!("{}-network-{}", attempt.call_id, Uuid::new_v4());
        let receiver = session
            .request_command_approval(crate::codex::CommandApprovalRequest {
                sub_id: attempt.turn_id.clone(),
                call_id: attempt.call_id.clone(),
                approval_id: Some(approval_id),
                command: attempt.command.clone(),
                cwd: attempt.cwd.clone(),
                reason: Some(format!(
                    "Network access to \"{host_display}\" is blocked by policy."
                )),
                network_approval_context: Some(NetworkApprovalContext {
                    host: host_display,
                    protocol,
                }),
                additional_permissions: None,
            })
            .await;
        let decision = receiver.await.unwrap_or(ReviewDecision::Denied);

        match decision {
            ReviewDecision::Approved => {
                let mut approved_hosts = attempt.approved_hosts.lock().await;
                approved_hosts.insert(host);
                NetworkDecision::Allow
            }
            ReviewDecision::ApprovedForSession => {
                let mut approved_hosts = self.session_approved_hosts.lock().await;
                approved_hosts.insert(host);
                NetworkDecision::Allow
            }
            ReviewDecision::Denied | ReviewDecision::Abort => {
                let mut denied = attempt.denied_by_user.lock().await;
                *denied = true;
                NetworkDecision::deny(REASON_NOT_ALLOWED)
            }
        }
    }
}

pub(crate) fn build_network_policy_decider(
    network_approval: Arc<NetworkApprovalService>,
    session: Arc<RwLock<std::sync::Weak<Session>>>,
) -> Arc<dyn NetworkPolicyDecider> {
    Arc::new(move |request: NetworkPolicyRequest| {
        let network_approval = Arc::clone(&network_approval);
        let session = Arc::clone(&session);
        async move {
            let Some(session) = session.read().await.upgrade() else {
                return NetworkDecision::ask(REASON_NOT_ALLOWED);
            };
            network_approval
                .handle_policy_request(session.as_ref(), request)
                .await
        }
    })
}
