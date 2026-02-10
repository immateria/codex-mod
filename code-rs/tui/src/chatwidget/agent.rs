use std::sync::Arc;

use code_core::protocol::Event;
use code_core::protocol::EventMsg;
use code_core::CodexConversation;
use code_core::ConversationManager;
use code_core::config::Config;
use code_core::protocol::Op;
use code_login::AuthManager;
use code_protocol::protocol::SessionSource;
use tokio::sync::mpsc::UnboundedReceiver;

use crate::app_event::AppEvent;
use crate::app_event_sender::AppEventSender;
use crate::chatwidget::BackgroundOrderTicket;

/// Spawn bootstrap + forwarding loops for a brand-new conversation session.
pub(crate) fn spawn_new_conversation_runtime(
    config: Config,
    app_event_tx: AppEventSender,
    auth_manager: Arc<AuthManager>,
    code_op_rx: UnboundedReceiver<Op>,
    ticket: BackgroundOrderTicket,
) {
    let ticket_for_submit = ticket.clone();

    tokio::spawn(async move {
        let app_event_tx_clone = app_event_tx;
        let mut code_op_rx = code_op_rx;
        let conversation_manager = ConversationManager::new(
            auth_manager.clone(),
            SessionSource::Cli,
        );
        let resume_path = config.experimental_resume.clone();
        let new_conversation = match resume_path {
            Some(path) => conversation_manager
                .resume_conversation_from_rollout(config.clone(), path, auth_manager.clone())
                .await,
            None => conversation_manager.new_conversation(config).await,
        };

        let new_conversation = match new_conversation {
            Ok(conv) => conv,
            Err(e) => {
                tracing::error!("failed to initialize conversation: {e}");
                app_event_tx_clone.send_background_event_with_ticket(
                    &ticket,
                    format!(
                        "❌ Failed to initialize model session: {e}.\n• Ensure an OpenAI API key is set (CODE_OPENAI_API_KEY / OPENAI_API_KEY) or run `code login`.\n• Also verify config.cwd is an absolute path."
                    ),
                );
                return;
            }
        };

        let event = Event {
            id: new_conversation.conversation_id.to_string(),
            event_seq: 0,
            msg: EventMsg::SessionConfigured(new_conversation.session_configured),
            order: None,
        };
        app_event_tx_clone.send(AppEvent::CodexEvent(event));

        let conversation = new_conversation.conversation;
        let conversation_clone = conversation.clone();
        let app_event_tx_submit = app_event_tx_clone.clone();
        let ticket_for_submit = ticket_for_submit.clone();

        tokio::spawn(async move {
            while let Some(op) = code_op_rx.recv().await {
                if let Err(e) = conversation_clone.submit(op).await {
                    tracing::error!("failed to submit op: {e}");
                    app_event_tx_submit.send_background_event_with_ticket(
                        &ticket_for_submit,
                        format!("⚠️ Failed to submit Op to core: {e}"),
                    );
                }
            }
        });

        while let Ok(event) = conversation.next_event().await {
            app_event_tx_clone.send(AppEvent::CodexEvent(event));
        }
    });

}

/// Spawn forwarding loops for a pre-existing conversation (forked session).
pub(crate) fn spawn_existing_conversation_runtime(
    conversation: Arc<CodexConversation>,
    session_configured: code_core::protocol::SessionConfiguredEvent,
    app_event_tx: AppEventSender,
    code_op_rx: UnboundedReceiver<Op>,
) {
    tokio::spawn(async move {
        let app_event_tx_clone = app_event_tx;
        let mut code_op_rx = code_op_rx;
        let event = Event {
            id: "fork".to_string(),
            event_seq: 0,
            msg: EventMsg::SessionConfigured(session_configured),
            order: None,
        };
        app_event_tx_clone.send(AppEvent::CodexEvent(event));

        let conversation_clone = conversation.clone();
        tokio::spawn(async move {
            while let Some(op) = code_op_rx.recv().await {
                let id = conversation_clone.submit(op).await;
                if let Err(e) = id {
                    tracing::error!("failed to submit op: {e}");
                }
            }
        });

        while let Ok(event) = conversation.next_event().await {
            app_event_tx_clone.send(AppEvent::CodexEvent(event));
        }
    });
}
