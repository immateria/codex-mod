#[cfg(target_os = "linux")]
use std::path::{Path, PathBuf};

#[cfg(target_os = "linux")]
use std::sync::{OnceLock, RwLock};

#[cfg(target_os = "linux")]
const CGROUP_MOUNT: &str = "/sys/fs/cgroup";

#[cfg(target_os = "linux")]
const EXEC_CGROUP_SUBDIR: &str = "code-exec";

#[cfg(target_os = "linux")]
const EXEC_CGROUP_OOM_SCORE_ADJ: &str = "500";

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ExecCgroupLimits {
    pub(crate) memory_max_bytes: Option<u64>,
    pub(crate) pids_max: Option<u64>,
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ExecLimitOverride {
    #[default]
    Auto,
    Disabled,
    Value(u64),
}

#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ExecCgroupLimitOverrides {
    pub(crate) memory_max_bytes: ExecLimitOverride,
    pub(crate) pids_max: ExecLimitOverride,
}

#[cfg(target_os = "linux")]
static EXEC_CGROUP_LIMIT_OVERRIDES: OnceLock<RwLock<ExecCgroupLimitOverrides>> = OnceLock::new();

#[cfg(target_os = "linux")]
pub(crate) fn set_exec_cgroup_limit_overrides(overrides: ExecCgroupLimitOverrides) {
    let lock = EXEC_CGROUP_LIMIT_OVERRIDES
        .get_or_init(|| RwLock::new(ExecCgroupLimitOverrides::default()));
    if let Ok(mut guard) = lock.write() {
        *guard = overrides;
    }
}

#[cfg(target_os = "linux")]
fn exec_cgroup_limit_overrides_snapshot() -> ExecCgroupLimitOverrides {
    let lock = EXEC_CGROUP_LIMIT_OVERRIDES
        .get_or_init(|| RwLock::new(ExecCgroupLimitOverrides::default()));
    lock.read().map(|guard| *guard).unwrap_or_default()
}

#[cfg(target_os = "linux")]
pub(crate) fn default_exec_memory_max_bytes() -> Option<u64> {
    match exec_cgroup_limit_overrides_snapshot().memory_max_bytes {
        ExecLimitOverride::Disabled => return None,
        ExecLimitOverride::Value(value) => return Some(value),
        ExecLimitOverride::Auto => {}
    }

    auto_exec_memory_max_bytes()
}

#[cfg(target_os = "linux")]
pub(crate) fn auto_exec_memory_max_bytes() -> Option<u64> {
    if let Ok(raw) = std::env::var("CODEX_EXEC_MEMORY_MAX_BYTES") {
        if let Ok(value) = raw.trim().parse::<u64>() {
            if value > 0 {
                return Some(value);
            }
        }
    }
    if let Ok(raw) = std::env::var("CODEX_EXEC_MEMORY_MAX_MB") {
        if let Ok(value) = raw.trim().parse::<u64>() {
            if value > 0 {
                return Some(value.saturating_mul(1024 * 1024));
            }
        }
    }

    let available = read_mem_available_bytes()?;
    // Leave headroom for the parent TUI + other background processes.
    // Keep the cap within a reasonable range so we still protect the parent
    // on larger machines.
    let sixty_percent = available.saturating_mul(60) / 100;
    let min = 512_u64.saturating_mul(1024 * 1024);
    let max = 4_u64.saturating_mul(1024 * 1024 * 1024);
    Some(sixty_percent.clamp(min, max))
}

#[cfg(target_os = "linux")]
fn default_exec_pids_max_for_cpus(cpus: u64) -> u64 {
    // Start small by default (protect the parent) but scale a bit with cores.
    // Clamp to keep it reasonable across small and large machines.
    cpus.saturating_mul(64).clamp(256, 4096)
}

#[cfg(target_os = "linux")]
pub(crate) fn default_exec_pids_max() -> Option<u64> {
    match exec_cgroup_limit_overrides_snapshot().pids_max {
        ExecLimitOverride::Disabled => return None,
        ExecLimitOverride::Value(value) => return Some(value),
        ExecLimitOverride::Auto => {}
    }

    auto_exec_pids_max()
}

#[cfg(target_os = "linux")]
pub(crate) fn auto_exec_pids_max() -> Option<u64> {
    if let Ok(raw) = std::env::var("CODEX_EXEC_PIDS_MAX") {
        if let Ok(value) = raw.trim().parse::<u64>() {
            if value >= 1 {
                return Some(value);
            }
        }
    }

    let cpus = std::thread::available_parallelism()
        .map(|n| n.get() as u64)
        .unwrap_or(4);
    Some(default_exec_pids_max_for_cpus(cpus))
}

#[cfg(target_os = "linux")]
fn read_mem_available_bytes() -> Option<u64> {
    let contents = std::fs::read_to_string("/proc/meminfo").ok()?;
    for line in contents.lines() {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("MemAvailable:") {
            let kb = rest
                .split_whitespace()
                .next()
                .and_then(|n| n.parse::<u64>().ok())?;
            return Some(kb.saturating_mul(1024));
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn is_cgroup_v2() -> bool {
    std::fs::metadata(Path::new(CGROUP_MOUNT).join("cgroup.controllers")).is_ok()
}

#[cfg(target_os = "linux")]
fn current_cgroup_relative() -> Option<PathBuf> {
    let contents = std::fs::read_to_string("/proc/self/cgroup").ok()?;
    for line in contents.lines() {
        if let Some(path) = line.strip_prefix("0::") {
            let trimmed = path.trim();
            if trimmed.is_empty() {
                return None;
            }
            return Some(PathBuf::from(trimmed.trim_start_matches('/')));
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn exec_cgroup_parent_abs() -> Option<PathBuf> {
    if !is_cgroup_v2() {
        return None;
    }
    let rel = current_cgroup_relative()?;
    Some(Path::new(CGROUP_MOUNT).join(rel).join(EXEC_CGROUP_SUBDIR))
}

#[cfg(target_os = "linux")]
pub(crate) fn exec_cgroup_abs_for_pid(pid: u32) -> Option<PathBuf> {
    exec_cgroup_parent_abs().map(|parent| parent.join(format!("pid-{pid}")))
}

#[cfg(target_os = "linux")]
fn best_effort_enable_memory_controller(parent: &Path) {
    let controllers = std::fs::read_to_string(parent.join("cgroup.controllers")).ok();
    if controllers.as_deref().unwrap_or_default().split_whitespace().all(|c| c != "memory") {
        return;
    }
    let subtree = parent.join("cgroup.subtree_control");
    let _ = std::fs::write(subtree, "+memory");
}

#[cfg(target_os = "linux")]
fn best_effort_enable_pids_controller(parent: &Path) {
    let controllers = std::fs::read_to_string(parent.join("cgroup.controllers")).ok();
    if controllers.as_deref().unwrap_or_default().split_whitespace().all(|c| c != "pids") {
        return;
    }
    let subtree = parent.join("cgroup.subtree_control");
    let _ = std::fs::write(subtree, "+pids");
}

#[cfg(target_os = "linux")]
fn best_effort_attach_pid_to_exec_cgroup_inner(
    pid: u32,
    limits: ExecCgroupLimits,
    set_self_oom_score_adj: bool,
) {
    let Some(parent) = exec_cgroup_parent_abs() else {
        return;
    };

    let _ = std::fs::create_dir_all(&parent);
    if limits.memory_max_bytes.is_some() {
        best_effort_enable_memory_controller(&parent);
    }
    if limits.pids_max.is_some() {
        best_effort_enable_pids_controller(&parent);
    }

    let cgroup_dir = parent.join(format!("pid-{pid}"));
    if std::fs::create_dir_all(&cgroup_dir).is_err() {
        return;
    }

    let mut attached = false;

    if let Some(memory_max_bytes) = limits.memory_max_bytes {
        let memory_max_path = cgroup_dir.join("memory.max");
        if memory_max_path.exists() {
            let _ = std::fs::write(&memory_max_path, memory_max_bytes.to_string());
            attached = true;

            let oom_group_path = cgroup_dir.join("memory.oom.group");
            if oom_group_path.exists() {
                let _ = std::fs::write(oom_group_path, "1");
            }

            if set_self_oom_score_adj {
                // Prefer killing the exec subtree first if the host does hit global OOM.
                let _ = std::fs::write("/proc/self/oom_score_adj", EXEC_CGROUP_OOM_SCORE_ADJ);
            }
        }
    }

    if let Some(pids_max) = limits.pids_max {
        let pids_max_path = cgroup_dir.join("pids.max");
        if pids_max_path.exists() {
            let _ = std::fs::write(&pids_max_path, pids_max.to_string());
            attached = true;
        }
    }

    if !attached {
        return;
    }

    let procs_path = cgroup_dir.join("cgroup.procs");
    if procs_path.exists() {
        let _ = std::fs::write(procs_path, pid.to_string());
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn best_effort_attach_self_to_exec_cgroup(pid: u32, limits: ExecCgroupLimits) {
    best_effort_attach_pid_to_exec_cgroup_inner(pid, limits, true);
}

#[cfg(target_os = "linux")]
pub(crate) fn best_effort_attach_pid_to_exec_cgroup(pid: u32, limits: ExecCgroupLimits) {
    best_effort_attach_pid_to_exec_cgroup_inner(pid, limits, false);
}

#[cfg(target_os = "linux")]
pub(crate) fn exec_cgroup_oom_killed(pid: u32) -> Option<bool> {
    let dir = exec_cgroup_abs_for_pid(pid)?;
    let contents = std::fs::read_to_string(dir.join("memory.events")).ok()?;
    for line in contents.lines() {
        let mut parts = line.split_whitespace();
        let Some(key) = parts.next() else {
            continue;
        };
        let Some(val) = parts.next() else {
            continue;
        };
        if key == "oom_kill" {
            if let Ok(parsed) = val.parse::<u64>() {
                return Some(parsed > 0);
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
pub(crate) fn exec_cgroup_memory_max_bytes(pid: u32) -> Option<u64> {
    let dir = exec_cgroup_abs_for_pid(pid)?;
    let raw = std::fs::read_to_string(dir.join("memory.max")).ok()?;
    let trimmed = raw.trim();
    if trimmed == "max" {
        return None;
    }
    trimmed.parse::<u64>().ok()
}

#[cfg(target_os = "linux")]
pub(crate) fn best_effort_cleanup_exec_cgroup(pid: u32) {
    let Some(dir) = exec_cgroup_abs_for_pid(pid) else {
        return;
    };
    // Only remove the per-pid directory. The parent container stays.
    let _ = std::fs::remove_dir(&dir);
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;

    #[test]
    fn default_exec_pids_max_for_cpus_clamps_low_and_high() {
        assert_eq!(default_exec_pids_max_for_cpus(1), 256);
        assert_eq!(default_exec_pids_max_for_cpus(8), 512);
        assert_eq!(default_exec_pids_max_for_cpus(64), 4096);
    }
}
