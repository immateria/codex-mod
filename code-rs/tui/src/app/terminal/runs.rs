use std::io::{Read, Write};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::FutureExt;
use portable_pty::{native_pty_system, CommandBuilder, PtyPair, PtySize};
use shlex::try_join;

use crate::app::state::{
    App,
    AppState,
    DEFAULT_PTY_COLS,
    DEFAULT_PTY_ROWS,
    TerminalRunState,
};
use crate::app_event::{AppEvent, TerminalRunController, TerminalRunEvent};

pub(super) fn start_terminal_run_inner(
    app: &mut App<'_>,
    id: u64,
    command: Vec<String>,
    display: Option<String>,
    controller: Option<TerminalRunController>,
) {
    if command.is_empty() {
        app.app_event_tx.send(AppEvent::TerminalChunk {
            id,
            chunk: b"Install command not resolved".to_vec(),
            _is_stderr: true,
        });
        app.app_event_tx.send(AppEvent::TerminalExit {
            id,
            exit_code: Some(1),
            _duration: Duration::from_millis(0),
        });
        return;
    }

    let joined_display = try_join(command.iter().map(String::as_str))
        .ok()
        .unwrap_or_else(|| command.join(" "));

    let display_line = display.unwrap_or(joined_display);

    if !display_line.trim().is_empty() {
        let line = format!("$ {display_line}\n");
        app.app_event_tx.send(AppEvent::TerminalChunk {
            id,
            chunk: line.into_bytes(),
            _is_stderr: false,
        });
    }

    let stored_command = command;
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    let (writer_tx_raw, writer_rx) = channel::<Vec<u8>>();
    let writer_tx_shared = Arc::new(Mutex::new(Some(writer_tx_raw)));
    let controller_clone = controller.clone();
    let cwd = app.config.cwd.clone();
    let controller_tx = controller.map(|c| c.tx);

    let (pty_rows, pty_cols) = match &app.app_state {
        AppState::Chat { widget } => widget
            .terminal_dimensions_hint()
            .unwrap_or((DEFAULT_PTY_ROWS, DEFAULT_PTY_COLS)),
        _ => (DEFAULT_PTY_ROWS, DEFAULT_PTY_COLS),
    };

    let pty_system = native_pty_system();
    let pair = match pty_system.openpty(PtySize {
        rows: pty_rows,
        cols: pty_cols,
        pixel_width: 0,
        pixel_height: 0,
    }) {
        Ok(pair) => pair,
        Err(err) => {
            let msg = format!("Failed to open PTY: {err}\n");
            app.app_event_tx.send(AppEvent::TerminalChunk {
                id,
                chunk: msg.clone().into_bytes(),
                _is_stderr: true,
            });
            if let Some(ref ctrl) = controller_tx {
                let _ = ctrl.send(TerminalRunEvent::Chunk {
                    data: msg.into_bytes(),
                    _is_stderr: true,
                });
                let _ = ctrl.send(TerminalRunEvent::Exit {
                    exit_code: Some(1),
                    _duration: Duration::from_millis(0),
                });
            }
            app.app_event_tx.send(AppEvent::TerminalExit {
                id,
                exit_code: Some(1),
                _duration: Duration::from_millis(0),
            });
            return;
        }
    };

    let PtyPair { master, slave } = pair;
    let master = Arc::new(Mutex::new(master));

    let writer = {
        let guard = match master.lock() {
            Ok(guard) => guard,
            Err(_) => {
                let msg = "Failed to acquire terminal writer: poisoned lock\n".to_string();
                app.app_event_tx.send(AppEvent::TerminalChunk {
                    id,
                    chunk: msg.clone().into_bytes(),
                    _is_stderr: true,
                });
                if let Some(ref ctrl) = controller_tx {
                    let _ = ctrl.send(TerminalRunEvent::Chunk {
                        data: msg.into_bytes(),
                        _is_stderr: true,
                    });
                    let _ = ctrl.send(TerminalRunEvent::Exit {
                        exit_code: Some(1),
                        _duration: Duration::from_millis(0),
                    });
                }
                app.app_event_tx.send(AppEvent::TerminalExit {
                    id,
                    exit_code: Some(1),
                    _duration: Duration::from_millis(0),
                });
                return;
            }
        };
        let result = guard.take_writer();
        drop(guard);
        match result {
            Ok(writer) => writer,
            Err(err) => {
                let msg = format!("Failed to acquire terminal writer: {err}\n");
                app.app_event_tx.send(AppEvent::TerminalChunk {
                    id,
                    chunk: msg.clone().into_bytes(),
                    _is_stderr: true,
                });
                if let Some(ref ctrl) = controller_tx {
                    let _ = ctrl.send(TerminalRunEvent::Chunk {
                        data: msg.into_bytes(),
                        _is_stderr: true,
                    });
                    let _ = ctrl.send(TerminalRunEvent::Exit {
                        exit_code: Some(1),
                        _duration: Duration::from_millis(0),
                    });
                }
                app.app_event_tx.send(AppEvent::TerminalExit {
                    id,
                    exit_code: Some(1),
                    _duration: Duration::from_millis(0),
                });
                return;
            }
        }
    };

    let reader = {
        let guard = match master.lock() {
            Ok(guard) => guard,
            Err(_) => {
                let msg = "Failed to read terminal output: poisoned lock\n".to_string();
                app.app_event_tx.send(AppEvent::TerminalChunk {
                    id,
                    chunk: msg.clone().into_bytes(),
                    _is_stderr: true,
                });
                if let Some(ref ctrl) = controller_tx {
                    let _ = ctrl.send(TerminalRunEvent::Chunk {
                        data: msg.into_bytes(),
                        _is_stderr: true,
                    });
                    let _ = ctrl.send(TerminalRunEvent::Exit {
                        exit_code: Some(1),
                        _duration: Duration::from_millis(0),
                    });
                }
                app.app_event_tx.send(AppEvent::TerminalExit {
                    id,
                    exit_code: Some(1),
                    _duration: Duration::from_millis(0),
                });
                return;
            }
        };
        let result = guard.try_clone_reader();
        drop(guard);
        match result {
            Ok(reader) => reader,
            Err(err) => {
                let msg = format!("Failed to read terminal output: {err}\n");
                app.app_event_tx.send(AppEvent::TerminalChunk {
                    id,
                    chunk: msg.clone().into_bytes(),
                    _is_stderr: true,
                });
                if let Some(ref ctrl) = controller_tx {
                    let _ = ctrl.send(TerminalRunEvent::Chunk {
                        data: msg.into_bytes(),
                        _is_stderr: true,
                    });
                    let _ = ctrl.send(TerminalRunEvent::Exit {
                        exit_code: Some(1),
                        _duration: Duration::from_millis(0),
                    });
                }
                app.app_event_tx.send(AppEvent::TerminalExit {
                    id,
                    exit_code: Some(1),
                    _duration: Duration::from_millis(0),
                });
                return;
            }
        }
    };

    let mut command_builder = CommandBuilder::new(stored_command[0].clone());
    for arg in &stored_command[1..] {
        command_builder.arg(arg);
    }
    command_builder.cwd(&cwd);

    let mut child = match slave.spawn_command(command_builder) {
        Ok(child) => child,
        Err(err) => {
            let msg = format!("Failed to spawn command: {err}\n");
            app.app_event_tx.send(AppEvent::TerminalChunk {
                id,
                chunk: msg.clone().into_bytes(),
                _is_stderr: true,
            });
            if let Some(ref ctrl) = controller_tx {
                let _ = ctrl.send(TerminalRunEvent::Chunk {
                    data: msg.into_bytes(),
                    _is_stderr: true,
                });
                let _ = ctrl.send(TerminalRunEvent::Exit {
                    exit_code: Some(1),
                    _duration: Duration::from_millis(0),
                });
            }
            app.app_event_tx.send(AppEvent::TerminalExit {
                id,
                exit_code: Some(1),
                _duration: Duration::from_millis(0),
            });
            return;
        }
    };

    let mut killer = child.clone_killer();

    let master_for_state = Arc::clone(&master);
    app.terminal_runs.insert(
        id,
        TerminalRunState {
            command: stored_command,
            display: display_line,
            cancel_tx: Some(cancel_tx),
            running: true,
            controller: controller_clone,
            writer_tx: Some(writer_tx_shared.clone()),
            pty: Some(master_for_state),
        },
    );

    let tx = app.app_event_tx.clone();
    let controller_tx_task = controller_tx;
    let master_for_task = Arc::clone(&master);
    let writer_tx_for_task = writer_tx_shared;
    tokio::spawn(async move {
        let start_time = Instant::now();
        let controller_tx = controller_tx_task;
        let _master = master_for_task;

        let writer_handle = tokio::task::spawn_blocking(move || {
            let mut writer = writer;
            while let Ok(bytes) = writer_rx.recv() {
                if writer.write_all(&bytes).is_err() {
                    break;
                }
                if writer.flush().is_err() {
                    break;
                }
            }
        });

        let tx_reader = tx.clone();
        let controller_tx_reader = controller_tx.clone();
        let reader_handle = tokio::task::spawn_blocking(move || {
            let mut buf = [0u8; 8192];
            let mut reader = reader;
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => {
                        let chunk = buf[..n].to_vec();
                        tx_reader.send(AppEvent::TerminalChunk {
                            id,
                            chunk: chunk.clone(),
                            _is_stderr: false,
                        });
                        if let Some(ref ctrl) = controller_tx_reader {
                            let _ = ctrl.send(TerminalRunEvent::Chunk {
                                data: chunk,
                                _is_stderr: false,
                            });
                        }
                    }
                    Err(err) => {
                        let msg = format!("Error reading terminal output: {err}\n");
                        tx_reader.send(AppEvent::TerminalChunk {
                            id,
                            chunk: msg.clone().into_bytes(),
                            _is_stderr: true,
                        });
                        if let Some(ref ctrl) = controller_tx_reader {
                            let _ = ctrl.send(TerminalRunEvent::Chunk {
                                data: msg.into_bytes(),
                                _is_stderr: true,
                            });
                        }
                        break;
                    }
                }
            }
        });

        let mut cancel_rx = cancel_rx.fuse();
        let mut cancel_triggered = false;
        let wait_handle = tokio::task::spawn_blocking(move || child.wait());
        futures::pin_mut!(wait_handle);
        let wait_status = loop {
            tokio::select! {
                res = &mut wait_handle => break res,
                res = &mut cancel_rx, if !cancel_triggered => {
                    if res.is_ok() {
                        cancel_triggered = true;
                        let _ = killer.kill();
                    }
                }
            }
        };

        {
            let mut guard = writer_tx_for_task
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.take();
        }

        let _ = reader_handle.await;
        let _ = writer_handle.await;

        let (exit_code, duration) = match wait_status {
            Ok(Ok(status)) => (Some(status.exit_code() as i32), start_time.elapsed()),
            Ok(Err(err)) => {
                let msg = format!("Process wait failed: {err}\n");
                tx.send(AppEvent::TerminalChunk {
                    id,
                    chunk: msg.clone().into_bytes(),
                    _is_stderr: true,
                });
                if let Some(ref ctrl) = controller_tx {
                    let data = msg.into_bytes();
                    let _ = ctrl.send(TerminalRunEvent::Chunk {
                        data,
                        _is_stderr: true,
                    });
                }
                (None, start_time.elapsed())
            }
            Err(err) => {
                let msg = format!("Process join failed: {err}\n");
                tx.send(AppEvent::TerminalChunk {
                    id,
                    chunk: msg.clone().into_bytes(),
                    _is_stderr: true,
                });
                if let Some(ref ctrl) = controller_tx {
                    let data = msg.into_bytes();
                    let _ = ctrl.send(TerminalRunEvent::Chunk {
                        data,
                        _is_stderr: true,
                    });
                }
                (None, start_time.elapsed())
            }
        };

        if let Some(ref ctrl) = controller_tx {
            let _ = ctrl.send(TerminalRunEvent::Exit {
                exit_code,
                _duration: duration,
            });
        }
        tx.send(AppEvent::TerminalExit {
            id,
            exit_code,
            _duration: duration,
        });
    });
}

