use std::io;

#[cfg(unix)]
pub(crate) fn kill_process_group(process_group_id: u32) -> io::Result<()> {
    use std::io::ErrorKind;

    let pgid = process_group_id as libc::pid_t;
    let self_pgid = unsafe { libc::getpgrp() };
    if pgid == self_pgid {
        // Never kill our own process group.
        return Ok(());
    }

    let result = unsafe { libc::killpg(pgid, libc::SIGKILL) };
    if result == -1 {
        let err = io::Error::last_os_error();
        if err.kind() != ErrorKind::NotFound {
            return Err(err);
        }
    }

    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn kill_process_group(_process_group_id: u32) -> io::Result<()> {
    Ok(())
}

