use code_core::CodexConversation;
use code_core::protocol::Event;
use code_core::protocol::EventMsg;
use code_core::protocol::Op;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use tracing::debug;
use tracing::error;
use tracing::info;

pub(super) fn start_event_stream(conversation: Arc<CodexConversation>) -> UnboundedReceiver<Event> {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Event>();
    spawn_event_bridge(conversation, tx);
    rx
}

fn spawn_event_bridge(conversation: Arc<CodexConversation>, tx: UnboundedSender<Event>) {
    tokio::spawn(async move {
        #[cfg(unix)]
        let mut sigterm_stream =
            match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
                Ok(stream) => Some(stream),
                Err(err) => {
                    tracing::warn!("failed to install SIGTERM handler: {err}");
                    None
                }
            };
        #[cfg(unix)]
        let mut sigterm_requested = false;

        loop {
            #[cfg(unix)]
            {
                if let Some(stream) = sigterm_stream.as_mut() {
                    tokio::select! {
                        _ = stream.recv() => {
                            tracing::debug!("SIGTERM received; requesting shutdown");
                            conversation.submit(Op::Shutdown).await.ok();
                            sigterm_requested = true;
                            break;
                        }
                        _ = tokio::signal::ctrl_c() => {
                            tracing::debug!("Keyboard interrupt");
                            conversation.submit(Op::Interrupt).await.ok();
                            break;
                        }
                        res = conversation.next_event() => match res {
                            Ok(event) => {
                                debug!("Received event: {event:?}");

                                let is_shutdown_complete = matches!(event.msg, EventMsg::ShutdownComplete);
                                if let Err(err) = tx.send(event) {
                                    error!("Error sending event: {err:?}");
                                    break;
                                }
                                if is_shutdown_complete {
                                    info!("Received shutdown event, exiting event loop.");
                                    break;
                                }
                            },
                            Err(err) => {
                                error!("Error receiving event: {err:?}");
                                break;
                            }
                        }
                    }
                } else {
                    tokio::select! {
                        _ = tokio::signal::ctrl_c() => {
                            tracing::debug!("Keyboard interrupt");
                            conversation.submit(Op::Interrupt).await.ok();
                            break;
                        }
                        res = conversation.next_event() => match res {
                            Ok(event) => {
                                debug!("Received event: {event:?}");

                                let is_shutdown_complete = matches!(event.msg, EventMsg::ShutdownComplete);
                                if let Err(err) = tx.send(event) {
                                    error!("Error sending event: {err:?}");
                                    break;
                                }
                                if is_shutdown_complete {
                                    info!("Received shutdown event, exiting event loop.");
                                    break;
                                }
                            },
                            Err(err) => {
                                error!("Error receiving event: {err:?}");
                                break;
                            }
                        }
                    }
                }
            }
            #[cfg(not(unix))]
            {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        tracing::debug!("Keyboard interrupt");
                        conversation.submit(Op::Interrupt).await.ok();
                        break;
                    }
                    res = conversation.next_event() => match res {
                        Ok(event) => {
                            debug!("Received event: {event:?}");

                            let is_shutdown_complete = matches!(event.msg, EventMsg::ShutdownComplete);
                            if let Err(err) = tx.send(event) {
                                error!("Error sending event: {err:?}");
                                break;
                            }
                            if is_shutdown_complete {
                                info!("Received shutdown event, exiting event loop.");
                                break;
                            }
                        },
                        Err(err) => {
                            error!("Error receiving event: {err:?}");
                            break;
                        }
                    }
                }
            }
        }
        #[cfg(unix)]
        drop(sigterm_stream);
        #[cfg(unix)]
        if sigterm_requested {
            unsafe {
                libc::raise(libc::SIGTERM);
            }
        }
    });
}
