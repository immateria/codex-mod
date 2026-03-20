use super::FrameTimer;
use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::thread_spawner;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::prelude::*;

struct SharedWriter {
    buffer: Arc<Mutex<Vec<u8>>>,
}

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut guard = self.buffer.lock().unwrap();
        guard.extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn capture_logs(level: LevelFilter, f: impl FnOnce()) -> String {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let make_writer = {
        let buffer = Arc::clone(&buffer);
        move || SharedWriter {
            buffer: Arc::clone(&buffer),
        }
    };

    let layer = tracing_subscriber::fmt::layer()
        .with_target(false)
        .with_ansi(false)
        .with_writer(make_writer)
        .with_filter(level);
    let subscriber = tracing_subscriber::registry().with(layer);

    tracing::dispatcher::with_default(&subscriber.into(), f);

    let guard = buffer.lock().unwrap();
    String::from_utf8_lossy(&guard).to_string()
}

fn count_occurrences(haystack: &str, needle: &str) -> usize {
    haystack.match_indices(needle).count()
}

fn saturate_background_threads() -> (Vec<std::thread::JoinHandle<()>>, Arc<AtomicBool>) {
    let stop = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();

    loop {
        let stop_flag = Arc::clone(&stop);
        match thread_spawner::spawn_lightweight("test-blocker", move || {
            while !stop_flag.load(Ordering::Relaxed) {
                std::thread::sleep(Duration::from_millis(10));
            }
        }) {
            Some(handle) => handles.push(handle),
            None => break,
        }
    }

    (handles, stop)
}

#[test]
fn frame_timer_spawn_rejection_logs_only_in_debug() {
    let (handles, stop) = saturate_background_threads();

    let (tx, _rx) = std::sync::mpsc::channel::<AppEvent>();
    let app_tx = AppEventSender::new(tx);

    let warn_timer = Arc::new(FrameTimer::new());
    let warn_output = capture_logs(LevelFilter::WARN, || {
        for _ in 0..8 {
            warn_timer.schedule(Duration::from_millis(1), app_tx.clone());
        }
    });

    assert_eq!(
        count_occurrences(&warn_output, "frame timer spawn rejected"),
        0,
        "expected no warn-level frame timer spam in normal logging"
    );

    let info_timer = Arc::new(FrameTimer::new());
    let info_output = capture_logs(LevelFilter::INFO, || {
        for _ in 0..8 {
            info_timer.schedule(Duration::from_millis(1), app_tx.clone());
        }
    });

    let count = count_occurrences(&info_output, "frame timer spawn rejected");
    assert!(
        count >= 1,
        "expected debug/info logs to include frame timer rejection"
    );
    assert!(count <= 1, "expected throttling to suppress repeats");

    stop.store(true, Ordering::Relaxed);
    for handle in handles {
        let _ = handle.join();
    }
}

