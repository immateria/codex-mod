use super::*;
use code_app_server_protocol::ThreadListResponse;

#[tokio::test]
async fn thread_list_v2_rejects_invalid_cursor() {
    let (
        mut processor,
        mut outgoing_rx,
        mut session,
        outbound_initialized,
        outbound_opted_out_notification_methods,
        temp_code_home,
    ) = setup_processor("thread-list-invalid-cursor");

    initialize_connection(
        &mut processor,
        &mut session,
        &outbound_initialized,
        &outbound_opted_out_notification_methods,
        &mut outgoing_rx,
    )
    .await;

    let request = JSONRPCRequest {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id: RequestId::Integer(2),
        method: "thread/list".to_string(),
        params: Some(json!({
            "cursor": "bad-cursor",
            "limit": 1
        })),
    };

    processor
        .process_request(
            ConnectionId(7),
            request,
            &mut session,
            &outbound_initialized,
            &outbound_opted_out_notification_methods,
        )
        .await;

    let error = expect_error(
        outgoing_rx.recv().await.expect("thread/list error envelope"),
        2,
    );
    assert_eq!(error.code, INVALID_REQUEST_ERROR_CODE);
    assert!(error.message.contains("invalid cursor"));

    let _ = std::fs::remove_dir_all(temp_code_home);
}

#[tokio::test]
async fn thread_list_v2_returns_response() {
    let (
        mut processor,
        mut outgoing_rx,
        mut session,
        outbound_initialized,
        outbound_opted_out_notification_methods,
        temp_code_home,
    ) = setup_processor("thread-list-response");

    initialize_connection(
        &mut processor,
        &mut session,
        &outbound_initialized,
        &outbound_opted_out_notification_methods,
        &mut outgoing_rx,
    )
    .await;

    let request = JSONRPCRequest {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id: RequestId::Integer(2),
        method: "thread/list".to_string(),
        params: Some(json!({
            "limit": 1
        })),
    };

    processor
        .process_request(
            ConnectionId(7),
            request,
            &mut session,
            &outbound_initialized,
            &outbound_opted_out_notification_methods,
        )
        .await;

    let response: ThreadListResponse = parse_response(
        outgoing_rx
            .recv()
            .await
            .expect("thread/list response envelope"),
        2,
    );
    assert!(response.data.len() <= 1);

    let _ = std::fs::remove_dir_all(temp_code_home);
}

#[tokio::test]
async fn thread_read_v2_rejects_invalid_thread_id() {
    let (
        mut processor,
        mut outgoing_rx,
        mut session,
        outbound_initialized,
        outbound_opted_out_notification_methods,
        temp_code_home,
    ) = setup_processor("thread-read-invalid-thread-id");

    initialize_connection(
        &mut processor,
        &mut session,
        &outbound_initialized,
        &outbound_opted_out_notification_methods,
        &mut outgoing_rx,
    )
    .await;

    let request = JSONRPCRequest {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id: RequestId::Integer(2),
        method: "thread/read".to_string(),
        params: Some(json!({
            "threadId": "not-a-uuid",
            "includeTurns": false
        })),
    };

    processor
        .process_request(
            ConnectionId(7),
            request,
            &mut session,
            &outbound_initialized,
            &outbound_opted_out_notification_methods,
        )
        .await;

    let error = expect_error(
        outgoing_rx.recv().await.expect("thread/read error envelope"),
        2,
    );
    assert_eq!(error.code, INVALID_REQUEST_ERROR_CODE);
    assert!(error.message.contains("invalid thread id"));

    let _ = std::fs::remove_dir_all(temp_code_home);
}

#[tokio::test]
async fn thread_read_v2_rejects_unknown_thread_id() {
    let (
        mut processor,
        mut outgoing_rx,
        mut session,
        outbound_initialized,
        outbound_opted_out_notification_methods,
        temp_code_home,
    ) = setup_processor("thread-read-unknown-thread-id");

    initialize_connection(
        &mut processor,
        &mut session,
        &outbound_initialized,
        &outbound_opted_out_notification_methods,
        &mut outgoing_rx,
    )
    .await;

    let request = JSONRPCRequest {
        jsonrpc: JSONRPC_VERSION.to_string(),
        id: RequestId::Integer(2),
        method: "thread/read".to_string(),
        params: Some(json!({
            "threadId": Uuid::new_v4().to_string(),
            "includeTurns": false
        })),
    };

    processor
        .process_request(
            ConnectionId(7),
            request,
            &mut session,
            &outbound_initialized,
            &outbound_opted_out_notification_methods,
        )
        .await;

    let error = expect_error(
        outgoing_rx.recv().await.expect("thread/read error envelope"),
        2,
    );
    assert_eq!(error.code, INVALID_REQUEST_ERROR_CODE);
    assert!(error.message.contains("thread not found"));

    let _ = std::fs::remove_dir_all(temp_code_home);
}
