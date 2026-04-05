use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use code_app_server_protocol::FsChangedNotification;
use code_app_server_protocol::FsUnwatchParams;
use code_app_server_protocol::FsUnwatchResponse;
use code_app_server_protocol::FsWatchParams;
use code_app_server_protocol::FsWatchResponse;
use code_app_server_protocol::ServerNotification;
use code_utils_absolute_path::AbsolutePathBuf;
use notify::Event;
use notify::EventKind;
use notify::RecommendedWatcher;
use notify::RecursiveMode;
use notify::Watcher;
use tokio::sync::Mutex as AsyncMutex;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::time::Instant;
use tokio::time::sleep_until;
use tracing::warn;
use uuid::Uuid;

use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::OutgoingMessageSender;
use crate::outgoing_message::OutgoingNotification;

const FS_CHANGED_NOTIFICATION_DEBOUNCE: Duration = Duration::from_millis(200);

struct DebouncedReceiver {
    rx: mpsc::UnboundedReceiver<notify::Result<Event>>,
    interval: Duration,
    changed_paths: HashSet<PathBuf>,
    next_allowance: Option<Instant>,
}

impl DebouncedReceiver {
    fn new(rx: mpsc::UnboundedReceiver<notify::Result<Event>>, interval: Duration) -> Self {
        Self {
            rx,
            interval,
            changed_paths: HashSet::new(),
            next_allowance: None,
        }
    }

    fn add_event(&mut self, event: &Event) {
        if !matches!(
            event.kind,
            EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
        ) {
            return;
        }

        self.changed_paths.extend(event.paths.iter().cloned());
    }

    async fn recv(&mut self) -> Option<Vec<PathBuf>> {
        while self.changed_paths.is_empty() {
            let event = match self.rx.recv().await? {
                Ok(event) => event,
                Err(err) => {
                    warn!("filesystem watch event error: {err}");
                    continue;
                }
            };
            self.add_event(&event);
        }

        let next_allowance = *self
            .next_allowance
            .get_or_insert_with(|| Instant::now() + self.interval);

        loop {
            tokio::select! {
                event = self.rx.recv() => match event {
                    Some(Ok(event)) => self.add_event(&event),
                    Some(Err(err)) => warn!("filesystem watch event error: {err}"),
                    None => break,
                },
                _ = sleep_until(next_allowance) => break,
            }
        }

        Some(self.changed_paths.drain().collect())
    }
}

#[derive(Clone)]
pub(crate) struct FsWatchManager {
    outgoing: Arc<OutgoingMessageSender>,
    watcher_supported: bool,
    state: Arc<AsyncMutex<FsWatchState>>,
}

#[derive(Default)]
struct FsWatchState {
    entries: HashMap<WatchKey, WatchEntry>,
}

struct WatchEntry {
    terminate_tx: Option<oneshot::Sender<oneshot::Sender<()>>>,
    _watcher: Option<RecommendedWatcher>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct WatchKey {
    connection_id: ConnectionId,
    watch_id: String,
}

impl FsWatchManager {
    pub(crate) fn new(outgoing: Arc<OutgoingMessageSender>) -> Self {
        // `notify::recommended_watcher` may fail (for example, on platforms without
        // filesystem notifications). Match upstream by falling back to a no-op manager.
        let watcher_supported = notify::recommended_watcher(|_: notify::Result<Event>| {}).is_ok();
        Self {
            outgoing,
            watcher_supported,
            state: Arc::new(AsyncMutex::new(FsWatchState::default())),
        }
    }

    pub(crate) async fn watch(
        &self,
        connection_id: ConnectionId,
        params: FsWatchParams,
    ) -> Result<FsWatchResponse, mcp_types::JSONRPCErrorError> {
        let watch_id = Uuid::now_v7().to_string();
        let watch_root = params.path.to_path_buf();

        let (terminate_tx, terminate_rx) = oneshot::channel::<oneshot::Sender<()>>();
        let mut watcher: Option<RecommendedWatcher> = None;

        if self.watcher_supported {
            let (tx, rx) = mpsc::unbounded_channel::<notify::Result<Event>>();
            let tx_clone = tx;
            match notify::recommended_watcher(move |res| {
                let _ = tx_clone.send(res);
            }) {
                Ok(mut next_watcher) => {
                    // Keep parity with upstream: if the path does not exist, the watch still
                    // succeeds but emits no notifications.
                    if watch_root.exists()
                        && let Err(err) = next_watcher.watch(&watch_root, RecursiveMode::NonRecursive)
                    {
                        warn!("failed to watch {}: {err}", watch_root.display());
                    }
                    watcher = Some(next_watcher);

                    let outgoing = Arc::clone(&self.outgoing);
                    let task_watch_id = watch_id.clone();
                    let task_watch_root = watch_root.clone();
                    tokio::spawn(async move {
                        let mut rx = DebouncedReceiver::new(rx, FS_CHANGED_NOTIFICATION_DEBOUNCE);
                        tokio::pin!(terminate_rx);

                        loop {
                            let event_paths = tokio::select! {
                                biased;
                                _ = &mut terminate_rx => break,
                                event = rx.recv() => match event {
                                    Some(paths) => paths,
                                    None => break,
                                },
                            };

                            let mut changed_paths = event_paths
                                .into_iter()
                                .filter_map(|path| {
                                    match AbsolutePathBuf::resolve_path_against_base(&path, &task_watch_root) {
                                        Ok(path) => Some(path),
                                        Err(err) => {
                                            warn!(
                                                "failed to normalize watch event path ({}) for {}: {err}",
                                                path.display(),
                                                task_watch_root.display(),
                                            );
                                            None
                                        }
                                    }
                                })
                                .collect::<Vec<_>>();
                            changed_paths.sort_by(|left, right| left.as_path().cmp(right.as_path()));
                            if !changed_paths.is_empty() {
                                send_server_notification_to_connection(
                                    outgoing.as_ref(),
                                    connection_id,
                                    ServerNotification::FsChanged(FsChangedNotification {
                                        watch_id: task_watch_id.clone(),
                                        changed_paths,
                                    }),
                                )
                                .await;
                            }
                        }
                    });
                }
                Err(err) => {
                    warn!("filesystem watch manager falling back to noop watcher: {err}");
                }
            }
        }

        self.state.lock().await.entries.insert(
            WatchKey {
                connection_id,
                watch_id: watch_id.clone(),
            },
            WatchEntry {
                terminate_tx: watcher.as_ref().map(|_| terminate_tx),
                _watcher: watcher,
            },
        );

        Ok(FsWatchResponse {
            watch_id,
            path: params.path,
        })
    }

    pub(crate) async fn unwatch(
        &self,
        connection_id: ConnectionId,
        params: FsUnwatchParams,
    ) -> Result<FsUnwatchResponse, mcp_types::JSONRPCErrorError> {
        let watch_key = WatchKey {
            connection_id,
            watch_id: params.watch_id,
        };
        let entry = self.state.lock().await.entries.remove(&watch_key);
        if let Some(entry) = entry
            && let Some(terminate_tx) = entry.terminate_tx
        {
            // Wait for the oneshot to be destroyed by the task to ensure that no notifications
            // are send after the unwatch response.
            let (done_tx, done_rx) = oneshot::channel();
            let _ = terminate_tx.send(done_tx);
            let _ = done_rx.await;
        }
        Ok(FsUnwatchResponse {})
    }

    pub(crate) async fn connection_closed(&self, connection_id: ConnectionId) {
        let keys: Vec<WatchKey> = {
            let state = self.state.lock().await;
            state
                .entries
                .keys()
                .filter(|key| key.connection_id == connection_id)
                .cloned()
                .collect()
        };

        if keys.is_empty() {
            return;
        }

        let mut state = self.state.lock().await;
        for key in keys {
            if let Some(entry) = state.entries.remove(&key)
                && let Some(terminate_tx) = entry.terminate_tx
            {
                let (done_tx, _done_rx) = oneshot::channel();
                let _ = terminate_tx.send(done_tx);
            }
        }
    }
}

async fn send_server_notification_to_connection(
    outgoing: &OutgoingMessageSender,
    connection_id: ConnectionId,
    notification: ServerNotification,
) {
    let method = notification.to_string();
    let params = match notification.to_params() {
        Ok(params) => Some(params),
        Err(err) => {
            warn!("failed to serialize notification params: {err}");
            None
        }
    };
    outgoing
        .send_notification_to_connection(connection_id, OutgoingNotification { method, params })
        .await;
}

