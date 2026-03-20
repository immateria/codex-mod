use std::time::Instant;

use super::TimingStats;

impl TimingStats {
    pub(in crate::app) fn on_key(&mut self) {
        self.key_events = self.key_events.saturating_add(1);
        self.last_key_event = Some(Instant::now());
        self.key_waiting_for_frame = true;
    }
    pub(in crate::app) fn on_redraw_begin(&mut self) { self.redraw_events = self.redraw_events.saturating_add(1); }
    pub(in crate::app) fn on_redraw_end(&mut self, started: Instant) {
        self.frames_drawn = self.frames_drawn.saturating_add(1);
        let dt = started.elapsed().as_nanos() as u64;
        self.draw_ns.push(dt);
        if self.key_waiting_for_frame {
            if let Some(t0) = self.last_key_event.take() {
                let d = t0.elapsed().as_nanos() as u64;
                self.key_to_frame_ns.push(d);
            }
            self.key_waiting_for_frame = false;
        }
    }
    fn pct(ns: &[u64], p: f64) -> f64 {
        if ns.is_empty() { return 0.0; }
        let mut v = ns.to_vec();
        v.sort_unstable();
        let idx = ((v.len() as f64 - 1.0) * p).round() as usize;
        (v[idx] as f64) / 1_000_000.0
    }
    pub(in crate::app) fn summarize(&self) -> String {
        let draw_p50 = Self::pct(&self.draw_ns, 0.50);
        let draw_p95 = Self::pct(&self.draw_ns, 0.95);
        let kf_p50 = Self::pct(&self.key_to_frame_ns, 0.50);
        let kf_p95 = Self::pct(&self.key_to_frame_ns, 0.95);
        format!(
            "app-timing: frames={}\n  redraw_events={} key_events={}\n  draw_ms: p50={:.2} p95={:.2}\n  key->frame_ms: p50={:.2} p95={:.2}",
            self.frames_drawn,
            self.redraw_events,
            self.key_events,
            draw_p50, draw_p95,
            kf_p50, kf_p95,
        )
    }
}
