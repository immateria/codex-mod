use crate::outgoing_message::ConnectionId;
use code_core::CodexConversation;
use code_protocol::ConversationId;
use std::collections::HashMap;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::Weak;
use tokio::sync::Mutex;
use tokio::sync::oneshot;

#[derive(Default)]
pub(crate) struct ThreadState {
    cancel_tx: Option<oneshot::Sender<()>>,
    listener_conversation: Option<Weak<CodexConversation>>,
    subscribed_connections: HashSet<ConnectionId>,
}

impl ThreadState {
    pub(crate) fn listener_matches(&self, conversation: &Arc<CodexConversation>) -> bool {
        self.listener_conversation
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|existing| Arc::ptr_eq(&existing, conversation))
    }

    pub(crate) fn set_listener(
        &mut self,
        cancel_tx: oneshot::Sender<()>,
        conversation: &Arc<CodexConversation>,
    ) {
        if let Some(previous) = self.cancel_tx.replace(cancel_tx) {
            let _ = previous.send(());
        }
        self.listener_conversation = Some(Arc::downgrade(conversation));
    }

    pub(crate) fn clear_listener(&mut self) {
        if let Some(cancel_tx) = self.cancel_tx.take() {
            let _ = cancel_tx.send(());
        }
        self.listener_conversation = None;
    }

    pub(crate) fn add_connection(&mut self, connection_id: ConnectionId) {
        self.subscribed_connections.insert(connection_id);
    }

    pub(crate) fn remove_connection(&mut self, connection_id: ConnectionId) {
        self.subscribed_connections.remove(&connection_id);
    }

    pub(crate) fn subscribed_connection_ids(&self) -> Vec<ConnectionId> {
        self.subscribed_connections.iter().copied().collect()
    }
}

#[derive(Default)]
pub(crate) struct ThreadStateManager {
    thread_states: HashMap<ConversationId, Arc<Mutex<ThreadState>>>,
    thread_ids_by_connection: HashMap<ConnectionId, HashSet<ConversationId>>,
}

impl ThreadStateManager {
    pub(crate) fn thread_state(&mut self, thread_id: ConversationId) -> Arc<Mutex<ThreadState>> {
        self.thread_states
            .entry(thread_id)
            .or_insert_with(|| Arc::new(Mutex::new(ThreadState::default())))
            .clone()
    }

    pub(crate) async fn ensure_connection_subscribed(
        &mut self,
        thread_id: ConversationId,
        connection_id: ConnectionId,
    ) -> Arc<Mutex<ThreadState>> {
        self.thread_ids_by_connection
            .entry(connection_id)
            .or_default()
            .insert(thread_id);

        let thread_state = self.thread_state(thread_id);
        thread_state.lock().await.add_connection(connection_id);
        thread_state
    }

    pub(crate) async fn remove_connection(&mut self, connection_id: ConnectionId) {
        let Some(thread_ids) = self.thread_ids_by_connection.remove(&connection_id) else {
            return;
        };

        for thread_id in thread_ids {
            if let Some(thread_state) = self.thread_states.get(&thread_id) {
                let mut guard = thread_state.lock().await;
                guard.remove_connection(connection_id);
                if guard.subscribed_connection_ids().is_empty() {
                    guard.clear_listener();
                }
            }
        }
    }
}

