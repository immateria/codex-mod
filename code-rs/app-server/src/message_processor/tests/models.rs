use super::*;
use code_app_server_protocol::ModelListResponse;

#[tokio::test]
async fn model_list_v2_rejects_invalid_cursor() {
    let (
        mut processor,
        mut outgoing_rx,
        mut session,
        outbound_initialized,
        outbound_opted_out_notification_methods,
        temp_code_home,
    ) = setup_processor("model-list-invalid-cursor");

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
        method: "model/list".to_string(),
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
        outgoing_rx.recv().await.expect("model/list error envelope"),
        2,
    );
    assert_eq!(error.code, INVALID_REQUEST_ERROR_CODE);
    assert!(error.message.contains("invalid cursor"));

    let _ = std::fs::remove_dir_all(temp_code_home);
}

#[tokio::test]
async fn model_list_v2_returns_paginated_response() {
    let (
        mut processor,
        mut outgoing_rx,
        mut session,
        outbound_initialized,
        outbound_opted_out_notification_methods,
        temp_code_home,
    ) = setup_processor("model-list-response");

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
        method: "model/list".to_string(),
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

    let response: ModelListResponse = parse_response(
        outgoing_rx
            .recv()
            .await
            .expect("model/list response envelope"),
        2,
    );
    assert!(response.data.len() <= 1);

    let _ = std::fs::remove_dir_all(temp_code_home);
}
