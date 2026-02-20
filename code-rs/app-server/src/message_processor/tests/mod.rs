use std::collections::HashSet;
use std::sync::Arc;
use std::sync::RwLock;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;

use super::ConnectionSessionState;
use super::MessageProcessor;
use super::map_config_service_error;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::outgoing_message::ConnectionId;
use crate::outgoing_message::OutgoingEnvelope;
use crate::outgoing_message::OutgoingMessage;
use crate::outgoing_message::OutgoingMessageSender;
use code_app_server_protocol::ConfigValueWriteParams;
use code_app_server_protocol::ConfigWriteErrorCode;
use code_app_server_protocol::MergeStrategy;
use code_core::config::Config;
use mcp_types::JSONRPCRequest;
use mcp_types::JSONRPC_VERSION;
use mcp_types::RequestId;
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio::sync::mpsc;
use uuid::Uuid;

mod mcp_status;
mod models;
mod threads;

#[tokio::test]
async fn initialize_applies_opt_out_notification_methods_per_connection() {
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<OutgoingEnvelope>(8);
    let outgoing = Arc::new(OutgoingMessageSender::new_with_routed_sender(outgoing_tx));
    let config = Arc::new(
        Config::load_with_cli_overrides(Vec::new(), code_core::config::ConfigOverrides::default())
            .expect("load default config"),
    );
    let mut processor = MessageProcessor::new(outgoing, None, config, Vec::new(), Vec::new());
    let mut session = ConnectionSessionState::default();
    let outbound_initialized = AtomicBool::new(false);
    let outbound_opted_out_notification_methods = RwLock::new(HashSet::new());

    let request = JSONRPCRequest {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id: RequestId::Integer(1),
        method: "initialize".to_string(),
        params: Some(json!({
            "clientInfo": {
                "name": "client-a",
                "version": "1.0.0"
            },
            "capabilities": {
                "experimentalApi": false,
                "optOutNotificationMethods": ["configWarning", "codex/event/session_configured"]
            }
        })),
    };

    processor
        .process_request(
            ConnectionId(42),
            request,
            &mut session,
            &outbound_initialized,
            &outbound_opted_out_notification_methods,
        )
        .await;

    assert!(session.initialized, "session should be initialized");
    assert!(
        outbound_initialized.load(Ordering::Acquire),
        "outbound initialized flag should be set"
    );

    {
        let opted_out = outbound_opted_out_notification_methods
            .read()
            .expect("read lock");
        assert!(opted_out.contains("configWarning"));
        assert!(opted_out.contains("codex/event/session_configured"));
    }

    let envelope = outgoing_rx.recv().await.expect("initialize response envelope");
    match envelope {
        OutgoingEnvelope::Broadcast { .. } => {}
        _ => panic!("expected initialize response to be emitted"),
    }
}

#[tokio::test]
async fn v2_requests_require_initialize() {
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<OutgoingEnvelope>(8);
    let outgoing = Arc::new(OutgoingMessageSender::new_with_routed_sender(outgoing_tx));
    let config = Arc::new(
        Config::load_with_cli_overrides(Vec::new(), code_core::config::ConfigOverrides::default())
            .expect("load default config"),
    );
    let mut processor = MessageProcessor::new(outgoing, None, config, Vec::new(), Vec::new());
    let mut session = ConnectionSessionState::default();
    let outbound_initialized = AtomicBool::new(false);
    let outbound_opted_out_notification_methods = RwLock::new(HashSet::new());

    let request = JSONRPCRequest {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id: RequestId::Integer(7),
        method: "config/read".to_string(),
        params: Some(json!({
            "includeLayers": false,
        })),
    };

    processor
        .process_request(
            ConnectionId(42),
            request,
            &mut session,
            &outbound_initialized,
            &outbound_opted_out_notification_methods,
        )
        .await;

    let envelope = outgoing_rx
        .recv()
        .await
        .expect("expected not-initialized error");
    match envelope {
        OutgoingEnvelope::Broadcast {
            message: OutgoingMessage::Error(error),
        } => {
            assert_eq!(error.id, RequestId::Integer(7));
            assert_eq!(error.error.message, "Not initialized");
        }
        _ => panic!("expected broadcast error response"),
    }
}

#[test]
fn config_write_rejects_unreadable_existing_path() {
    let (outgoing_tx, _outgoing_rx) = mpsc::channel::<OutgoingEnvelope>(8);
    let outgoing = Arc::new(OutgoingMessageSender::new_with_routed_sender(outgoing_tx));

    let mut config =
        Config::load_with_cli_overrides(Vec::new(), code_core::config::ConfigOverrides::default())
            .expect("load default config");
    let temp_code_home = std::env::temp_dir().join(format!(
        "code-app-server-message-processor-{}",
        Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_code_home).expect("create temp code home");
    let config_toml_path = temp_code_home.join("config.toml");
    std::fs::create_dir_all(&config_toml_path).expect("create unreadable config path");
    config.code_home = temp_code_home.clone();

    let processor = MessageProcessor::new(
        outgoing,
        None,
        Arc::new(config),
        Vec::new(),
        Vec::new(),
    );
    let result = processor.config_service.write_value(ConfigValueWriteParams {
        key_path: "model".to_string(),
        value: json!("o3"),
        merge_strategy: MergeStrategy::Replace,
        file_path: None,
        expected_version: None,
    });

    let err = result.expect_err("write should fail when reading config path fails");
    let mapped = map_config_service_error(err);
    assert!(mapped.message.contains("Unable to read config file"));
    assert_eq!(
        mapped.data,
        Some(json!({
            "config_write_error_code": ConfigWriteErrorCode::ConfigValidationError,
        }))
    );

    let _ = std::fs::remove_dir_all(temp_code_home);
}

pub(super) fn setup_processor(
    test_name: &str,
) -> (
    MessageProcessor,
    mpsc::Receiver<OutgoingEnvelope>,
    ConnectionSessionState,
    AtomicBool,
    RwLock<HashSet<String>>,
    std::path::PathBuf,
) {
    let (outgoing_tx, outgoing_rx) = mpsc::channel::<OutgoingEnvelope>(16);
    let outgoing = Arc::new(OutgoingMessageSender::new_with_routed_sender(outgoing_tx));

    let mut config =
        Config::load_with_cli_overrides(Vec::new(), code_core::config::ConfigOverrides::default())
            .expect("load default config");
    let temp_code_home = std::env::temp_dir().join(format!(
        "code-app-server-message-processor-{test_name}-{}",
        Uuid::new_v4()
    ));
    std::fs::create_dir_all(&temp_code_home).expect("create temp code home");
    config.code_home = temp_code_home.clone();

    (
        MessageProcessor::new(outgoing, None, Arc::new(config), Vec::new(), Vec::new()),
        outgoing_rx,
        ConnectionSessionState::default(),
        AtomicBool::new(false),
        RwLock::new(HashSet::new()),
        temp_code_home,
    )
}

pub(super) async fn initialize_connection(
    processor: &mut MessageProcessor,
    session: &mut ConnectionSessionState,
    outbound_initialized: &AtomicBool,
    outbound_opted_out_notification_methods: &RwLock<HashSet<String>>,
    outgoing_rx: &mut mpsc::Receiver<OutgoingEnvelope>,
) {
    let request = JSONRPCRequest {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id: RequestId::Integer(1),
        method: "initialize".to_string(),
        params: Some(json!({
            "clientInfo": {
                "name": "client-tests",
                "version": "1.0.0"
            },
            "capabilities": {
                "experimentalApi": true
            }
        })),
    };

    processor
        .process_request(
            ConnectionId(7),
            request,
            session,
            outbound_initialized,
            outbound_opted_out_notification_methods,
        )
        .await;

    let envelope = outgoing_rx
        .recv()
        .await
        .expect("initialize response envelope");
    match envelope {
        OutgoingEnvelope::Broadcast {
            message: OutgoingMessage::Response(_),
        } => {}
        _ => panic!("expected initialize response"),
    }
}

pub(super) fn parse_response<T: DeserializeOwned>(envelope: OutgoingEnvelope, expected_id: i64) -> T {
    match envelope {
        OutgoingEnvelope::Broadcast {
            message: OutgoingMessage::Response(response),
        } => {
            assert_eq!(response.id, RequestId::Integer(expected_id));
            serde_json::from_value(response.result).expect("response payload should deserialize")
        }
        _ => panic!("expected response envelope"),
    }
}

pub(super) fn expect_error(
    envelope: OutgoingEnvelope,
    expected_id: i64,
) -> mcp_types::JSONRPCErrorError {
    match envelope {
        OutgoingEnvelope::Broadcast {
            message: OutgoingMessage::Error(error),
        } => {
            assert_eq!(error.id, RequestId::Integer(expected_id));
            error.error
        }
        _ => panic!("expected error envelope"),
    }
}
