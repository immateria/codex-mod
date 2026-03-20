use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::thread_spawner;

use super::{
    now_epoch_secs,
    AppEvent,
    AppEventSender,
    FrameTimer,
    FrameTimerState,
    FRAME_TIMER_LOG_THROTTLE_SECS,
};

impl FrameTimer {
    pub(in crate::app) fn new() -> Self {
        Self {
            state: Mutex::new(FrameTimerState {
                deadlines: BinaryHeap::new(),
                worker_running: false,
            }),
            cv: Condvar::new(),
            last_limit_log_secs: AtomicU64::new(0),
            suppressed_limit_logs: AtomicUsize::new(0),
        }
    }

    fn log_spawn_rejection(&self, drained: usize) {
        let now = now_epoch_secs();
        let last = self.last_limit_log_secs.load(Ordering::Relaxed);
        if now.saturating_sub(last) >= FRAME_TIMER_LOG_THROTTLE_SECS
            && self
                .last_limit_log_secs
                .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            let suppressed = self.suppressed_limit_logs.swap(0, Ordering::Relaxed);
            tracing::info!(
                drained_deadlines = drained,
                suppressed,
                "frame timer spawn rejected: background thread limit reached; flushed deadlines"
            );
        } else {
            self.suppressed_limit_logs.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub(in crate::app) fn schedule(self: &Arc<Self>, duration: Duration, tx: AppEventSender) {
        let deadline = Instant::now() + duration;
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.deadlines.push(Reverse(deadline));
        let should_spawn = if !state.worker_running {
            state.worker_running = true;
            true
        } else {
            false
        };
        self.cv.notify_one();
        drop(state);

        if should_spawn {
            let timer = Arc::clone(self);
            let tx_for_thread = tx.clone();
            if thread_spawner::spawn_lightweight("frame-timer", move || timer.run(tx_for_thread))
                .is_none()
            {
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                state.worker_running = false;
                let drained = state.deadlines.len();
                state.deadlines.clear();
                drop(state);
                for _ in 0..drained.max(1) {
                    tx.send(AppEvent::RequestRedraw);
                }
                self.log_spawn_rejection(drained);
            }
        }
    }

    fn run(self: Arc<Self>, tx: AppEventSender) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        loop {
            let deadline = match state.deadlines.peek().copied() {
                Some(Reverse(deadline)) => deadline,
                None => {
                    state.worker_running = false;
                    break;
                }
            };

            let now = Instant::now();
            if deadline <= now {
                state.deadlines.pop();
                drop(state);
                tx.send(AppEvent::RequestRedraw);
                state = self
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                continue;
            }

            let wait_dur = deadline.saturating_duration_since(now);
            let (new_state, result) = self
                .cv
                .wait_timeout(state, wait_dur)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state = new_state;

            if result.timed_out() {
                continue;
            }
        }
    }
}
