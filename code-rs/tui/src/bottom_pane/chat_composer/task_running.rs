use super::*;

impl ChatComposer {
    pub fn set_task_running(&mut self, running: bool) {
        self.is_task_running = running;

        if running {
            // Start animation thread if not already running
            if self.animation_running.is_none() {
                let animation_flag = Arc::new(AtomicBool::new(true));
                let animation_flag_clone = Arc::clone(&animation_flag);
                let app_event_tx_clone = self.app_event_tx.clone();

                // Drive redraws at the spinner's native cadence with a
                // phase‑aligned, monotonic scheduler to minimize drift and
                // reduce perceived frame skipping under load. We purposely
                // avoid very small intervals to keep CPU impact low.
                let fallback_tx = self.app_event_tx.clone();
                if let Some(handle) = thread_spawner::spawn_lightweight("composer-anim", move || {
                    use std::time::Instant;
                    // Default to ~120ms if spinner state is not yet initialized
                    let default_ms: u64 = 120;
                    // Clamp to a sane floor so we never busy loop if a custom spinner
                    // has an extremely small interval configured.
                    let min_ms: u64 = 60; // ~16 FPS upper bound for this thread

                    // Determine the target period. If the user changes the spinner
                    // while running, we'll still get correct visual output because
                    // frames are time‑based at render; this cadence simply requests
                    // redraws.
                    let period_ms = crate::spinner::current_spinner()
                        .interval_ms
                        .max(min_ms)
                        .max(1);
                    let period = Duration::from_millis(period_ms); // fallback uses default below if needed

                    let mut next = Instant::now()
                        .checked_add(if period_ms == 0 { Duration::from_millis(default_ms) } else { period })
                        .unwrap_or_else(Instant::now);

                    while animation_flag_clone.load(Ordering::Acquire) {
                        let now = Instant::now();
                        if now < next {
                            let sleep_dur = next - now;
                            thread::sleep(sleep_dur);
                        } else {
                            // If we're late (system busy), request a redraw immediately.
                            app_event_tx_clone.send(crate::app_event::AppEvent::RequestRedraw);
                            // Step the schedule forward by whole periods to avoid
                            // bursty catch‑up redraws.
                            let mut target = next;
                            while target <= now {
                                if let Some(t) = target.checked_add(period) { target = t; } else { break; }
                            }
                            next = target;
                        }
                    }
                }) {
                    self.animation_running = Some(AnimationThread {
                        running: animation_flag,
                        handle,
                    });
                } else {
                    fallback_tx.send(crate::app_event::AppEvent::RequestRedraw);
                }
            }
        } else {
            // Stop animation thread
            if let Some(animation_thread) = self.animation_running.take() {
                animation_thread.stop();
            }
        }
    }
}

impl Drop for ChatComposer {
    fn drop(&mut self) {
        if let Some(animation_thread) = self.animation_running.take() {
            animation_thread.stop();
        }
    }
}

