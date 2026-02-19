use std::num::NonZero;
use std::num::NonZeroUsize;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

use code_app_server_protocol::FuzzyFileSearchSessionCompletedNotification;
use code_app_server_protocol::FuzzyFileSearchSessionUpdatedNotification;
use code_file_search as file_search;
use code_protocol::mcp_protocol::FuzzyFileSearchResult;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tracing::warn;

use crate::outgoing_message::OutgoingMessageSender;
use crate::outgoing_message::OutgoingNotification;

const LIMIT_PER_ROOT: usize = 50;
const MAX_THREADS: usize = 12;
const COMPUTE_INDICES: bool = true;

pub(crate) async fn run_fuzzy_file_search(
    query: String,
    roots: Vec<String>,
    cancellation_flag: Arc<AtomicBool>,
) -> Vec<FuzzyFileSearchResult> {
    if roots.is_empty() {
        return Vec::new();
    }

    #[expect(clippy::expect_used)]
    let limit_per_root =
        NonZero::new(LIMIT_PER_ROOT).expect("LIMIT_PER_ROOT should be a valid non-zero usize");

    let cores = std::thread::available_parallelism()
        .map(std::num::NonZero::get)
        .unwrap_or(1);
    let threads = cores.min(MAX_THREADS);
    let threads_per_root = (threads / roots.len()).max(1);
    let threads = NonZero::new(threads_per_root).unwrap_or(NonZeroUsize::MIN);

    let mut files: Vec<FuzzyFileSearchResult> = Vec::new();
    let mut join_set = JoinSet::new();

    for root in roots {
        let search_dir = PathBuf::from(&root);
        let query = query.clone();
        let cancel_flag = cancellation_flag.clone();
        join_set.spawn_blocking(move || {
            match file_search::run(
                query.as_str(),
                limit_per_root,
                &search_dir,
                Vec::new(),
                threads,
                cancel_flag,
                COMPUTE_INDICES,
            ) {
                Ok(res) => Ok((root, res)),
                Err(err) => Err((root, err)),
            }
        });
    }

    while let Some(res) = join_set.join_next().await {
        match res {
            Ok(Ok((root, res))) => {
                for m in res.matches {
                    let path = m.path;
                    //TODO(shijie): Move file name generation to file_search lib.
                    let file_name = Path::new(&path)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| path.clone());
                    let result = FuzzyFileSearchResult {
                        root: root.clone(),
                        path,
                        file_name,
                        score: m.score,
                        indices: m.indices,
                    };
                    files.push(result);
                }
            }
            Ok(Err((root, err))) => {
                warn!("fuzzy-file-search in dir '{root}' failed: {err}");
            }
            Err(err) => {
                warn!("fuzzy-file-search join_next failed: {err}");
            }
        }
    }

    files.sort_by(file_search::cmp_by_score_desc_then_path_asc::<
        FuzzyFileSearchResult,
        _,
        _,
    >(|f| f.score, |f| f.path.as_str()));

    files
}

pub(crate) struct FuzzyFileSearchSession {
    session_id: String,
    roots: Vec<String>,
    outgoing: Arc<OutgoingMessageSender>,
    state: Arc<SessionState>,
}

struct SessionState {
    canceled: AtomicBool,
    generation: AtomicU64,
    current_cancel_flag: Mutex<Option<Arc<AtomicBool>>>,
}

impl FuzzyFileSearchSession {
    pub(crate) async fn update_query(&self, query: String) {
        if self.state.canceled.load(Ordering::Relaxed) {
            return;
        }

        let generation = self.state.generation.fetch_add(1, Ordering::Relaxed) + 1;
        let cancel_flag = Arc::new(AtomicBool::new(false));
        {
            let mut current = self.state.current_cancel_flag.lock().await;
            if let Some(existing) = current.take() {
                existing.store(true, Ordering::Relaxed);
            }
            *current = Some(cancel_flag.clone());
        }

        let files = if query.is_empty() {
            Vec::new()
        } else {
            run_fuzzy_file_search(query.clone(), self.roots.clone(), cancel_flag.clone()).await
        };

        if self.state.canceled.load(Ordering::Relaxed)
            || cancel_flag.load(Ordering::Relaxed)
            || generation != self.state.generation.load(Ordering::Relaxed)
        {
            return;
        }

        let params = FuzzyFileSearchSessionUpdatedNotification {
            session_id: self.session_id.clone(),
            query,
            files: files.into_iter().map(map_result).collect(),
        };
        if let Ok(params) = serde_json::to_value(params) {
            self.outgoing
                .send_notification(OutgoingNotification {
                    method: "fuzzyFileSearch/sessionUpdated".to_string(),
                    params: Some(params),
                })
                .await;
        }
    }

    pub(crate) async fn stop(&self) {
        if self.state.canceled.swap(true, Ordering::Relaxed) {
            return;
        }

        {
            let mut current = self.state.current_cancel_flag.lock().await;
            if let Some(existing) = current.take() {
                existing.store(true, Ordering::Relaxed);
            }
        }

        let params = FuzzyFileSearchSessionCompletedNotification {
            session_id: self.session_id.clone(),
        };
        if let Ok(params) = serde_json::to_value(params) {
            self.outgoing
                .send_notification(OutgoingNotification {
                    method: "fuzzyFileSearch/sessionCompleted".to_string(),
                    params: Some(params),
                })
                .await;
        }
    }
}

impl Drop for FuzzyFileSearchSession {
    fn drop(&mut self) {
        self.state.canceled.store(true, Ordering::Relaxed);
        if let Ok(mut current) = self.state.current_cancel_flag.try_lock()
            && let Some(existing) = current.take()
        {
            existing.store(true, Ordering::Relaxed);
        }
    }
}

pub(crate) fn start_fuzzy_file_search_session(
    session_id: String,
    roots: Vec<String>,
    outgoing: Arc<OutgoingMessageSender>,
) -> anyhow::Result<FuzzyFileSearchSession> {
    if session_id.is_empty() {
        anyhow::bail!("session id must not be empty");
    }

    Ok(FuzzyFileSearchSession {
        session_id,
        roots,
        outgoing,
        state: Arc::new(SessionState {
            canceled: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            current_cancel_flag: Mutex::new(None),
        }),
    })
}

fn map_result(value: FuzzyFileSearchResult) -> code_app_server_protocol::FuzzyFileSearchResult {
    code_app_server_protocol::FuzzyFileSearchResult {
        root: value.root,
        path: value.path,
        file_name: value.file_name,
        score: value.score,
        indices: value.indices,
    }
}
