use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use anyhow::anyhow;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::{Instant, sleep_until, timeout};

#[derive(Debug)]
pub(crate) struct McpRateLimiter {
    semaphore: Arc<Semaphore>,
    min_interval: Duration,
    next_start: Mutex<Instant>,
    queue_timeout: Option<Duration>,
    max_queue_depth: Option<u32>,
    queued: AtomicUsize,
}

impl McpRateLimiter {
    pub(crate) fn new(
        max_concurrent: u32,
        min_interval: Option<Duration>,
        queue_timeout: Option<Duration>,
        max_queue_depth: Option<u32>,
    ) -> Arc<Self> {
        Arc::new(Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent.max(1) as usize)),
            min_interval: min_interval.unwrap_or(Duration::ZERO),
            next_start: Mutex::new(Instant::now()),
            queue_timeout,
            max_queue_depth,
            queued: AtomicUsize::new(0),
        })
    }

    pub(crate) fn min_interval(&self) -> Duration { self.min_interval }

    pub(crate) fn queue_timeout(&self) -> Option<Duration> { self.queue_timeout }

    pub(crate) fn lock_next_start(&self) -> std::sync::MutexGuard<'_, Instant> {
        match self.next_start.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub(crate) async fn acquire(self: &Arc<Self>) -> Result<McpLimiterGuard> {
        let queued_guard = QueuedGuard::enter(Arc::clone(self))?;

        let entered_at = Instant::now();
        let permit = if let Some(queue_timeout) = self.queue_timeout {
            match timeout(queue_timeout, Arc::clone(&self.semaphore).acquire_owned()).await {
                Ok(permit) => permit.map_err(|err| anyhow!("failed to acquire MCP permit: {err}"))?,
                Err(_) => return Err(anyhow!("timed out waiting for MCP permit")),
            }
        } else {
            Arc::clone(&self.semaphore)
                .acquire_owned()
                .await
                .map_err(|err| anyhow!("failed to acquire MCP permit: {err}"))?
        };

        Ok(McpLimiterGuard {
            limiter: Arc::clone(self),
            _queued_guard: queued_guard,
            _permit: permit,
            entered_at,
        })
    }
}

pub(crate) struct McpLimiterGuard {
    limiter: Arc<McpRateLimiter>,
    _queued_guard: QueuedGuard,
    _permit: OwnedSemaphorePermit,
    entered_at: Instant,
}

impl McpLimiterGuard {
    pub(crate) fn limiter(&self) -> &Arc<McpRateLimiter> { &self.limiter }

    pub(crate) fn entered_at(&self) -> Instant { self.entered_at }
}

pub(crate) async fn acquire_and_schedule(
    server: &Arc<McpRateLimiter>,
    tool: Option<&Arc<McpRateLimiter>>,
) -> Result<(McpLimiterGuard, Option<McpLimiterGuard>)> {
    let server_guard = server.acquire().await?;
    let tool_guard = match tool {
        Some(limiter) => Some(limiter.acquire().await?),
        None => None,
    };

    let now = Instant::now();
    let scheduled = if let Some(tool_guard) = tool_guard.as_ref() {
        // Lock order matters to avoid deadlocks: always lock server first.
        let mut server_next = server_guard.limiter().lock_next_start();
        let mut tool_next = tool_guard.limiter().lock_next_start();
        let scheduled = (*server_next).max((*tool_next).max(now));
        *server_next = scheduled + server_guard.limiter().min_interval();
        *tool_next = scheduled + tool_guard.limiter().min_interval();
        scheduled
    } else {
        let mut server_next = server_guard.limiter().lock_next_start();
        let scheduled = (*server_next).max(now);
        *server_next = scheduled + server_guard.limiter().min_interval();
        scheduled
    };

    if let Some(queue_timeout) = server_guard.limiter().queue_timeout() {
        let waited = scheduled
            .saturating_duration_since(server_guard.entered_at());
        if waited > queue_timeout {
            return Err(anyhow!("MCP tool call queue timeout exceeded"));
        }
    }

    if scheduled > now {
        sleep_until(scheduled).await;
    }

    Ok((server_guard, tool_guard))
}

struct QueuedGuard {
    limiter: Arc<McpRateLimiter>,
}

impl QueuedGuard {
    fn enter(limiter: Arc<McpRateLimiter>) -> Result<Self> {
        let next = limiter.queued.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(max) = limiter.max_queue_depth
            && next > max as usize
        {
            limiter.queued.fetch_sub(1, Ordering::SeqCst);
            return Err(anyhow!(
                "MCP queue depth exceeded (max {max})"
            ));
        }
        Ok(Self { limiter })
    }
}

impl Drop for QueuedGuard {
    fn drop(&mut self) {
        self.limiter.queued.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn max_concurrent_blocks_second_call() {
        let limiter = McpRateLimiter::new(
            1,
            None,
            None,
            None,
        );

        let (first, _tool) =
            acquire_and_schedule(&limiter, None).await.expect("first acquire");
        let limiter2 = Arc::clone(&limiter);
        let second = tokio::spawn(async move { acquire_and_schedule(&limiter2, None).await });
        tokio::task::yield_now().await;
        assert!(!second.is_finished());

        drop(first);
        let _second = second.await.expect("join").expect("second acquire");
    }

    #[tokio::test(start_paused = true)]
    async fn min_interval_delays_second_call() {
        let limiter = McpRateLimiter::new(
            10,
            Some(Duration::from_secs(1)),
            None,
            None,
        );

        let (_first, _tool) =
            acquire_and_schedule(&limiter, None).await.expect("first");

        let limiter2 = Arc::clone(&limiter);
        let second = tokio::spawn(async move { acquire_and_schedule(&limiter2, None).await });
        tokio::task::yield_now().await;
        assert!(!second.is_finished());

        tokio::time::advance(Duration::from_secs(1)).await;
        let _second = second.await.expect("join").expect("second");
    }

    #[tokio::test(start_paused = true)]
    async fn queue_timeout_fails_when_delay_exceeds() {
        let limiter = McpRateLimiter::new(
            10,
            Some(Duration::from_secs(10)),
            Some(Duration::from_secs(1)),
            None,
        );

        let (_first, _tool) =
            acquire_and_schedule(&limiter, None).await.expect("first");

        let limiter2 = Arc::clone(&limiter);
        let second = tokio::spawn(async move { acquire_and_schedule(&limiter2, None).await });
        tokio::task::yield_now().await;
        assert!(second.is_finished());

        let result = second.await.expect("join");
        assert!(result.is_err());
    }
}
