use super::*;
use code_app_server_protocol::ListMcpServerStatusResponse;

#[tokio::test]
async fn mcp_server_status_list_v2_rejects_invalid_cursor() {
    let (
        mut processor,
        mut outgoing_rx,
        mut session,
        outbound_initialized,
        outbound_opted_out_notification_methods,
        temp_code_home,
    ) = setup_processor("mcp-status-list-invalid-cursor");

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
        method: "mcpServerStatus/list".to_string(),
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
        outgoing_rx
            .recv()
            .await
            .expect("mcpServerStatus/list error envelope"),
        2,
    );
    assert_eq!(error.code, INVALID_REQUEST_ERROR_CODE);
    assert!(error.message.contains("invalid cursor"));

    let _ = std::fs::remove_dir_all(temp_code_home);
}

#[tokio::test]
async fn mcp_server_status_list_v2_returns_response() {
    let (
        mut processor,
        mut outgoing_rx,
        mut session,
        outbound_initialized,
        outbound_opted_out_notification_methods,
        temp_code_home,
    ) = setup_processor("mcp-status-list-response");

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
        method: "mcpServerStatus/list".to_string(),
        params: Some(json!({
            "limit": 10
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

    let response: ListMcpServerStatusResponse = parse_response(
        outgoing_rx
            .recv()
            .await
            .expect("mcpServerStatus/list response envelope"),
        2,
    );
    assert!(response.next_cursor.is_none());
    assert!(response.data.is_empty());

    let _ = std::fs::remove_dir_all(temp_code_home);
}

#[tokio::test]
async fn mcp_server_status_list_v2_includes_extended_fields_for_disabled_server() {
    let (
        mut processor,
        mut outgoing_rx,
        mut session,
        outbound_initialized,
        outbound_opted_out_notification_methods,
        temp_code_home,
    ) = setup_processor("mcp-status-list-extended-fields");

    let config_path = temp_code_home.join("config.toml");
    std::fs::write(
        &config_path,
        r#"[mcp_servers_disabled.disabled_stdio]
command = "echo"
args = ["hello"]
startup_timeout_sec = 12.5
tool_timeout_sec = 33
disabled_tools = ["zeta", "alpha", "alpha"]
"#,
    )
    .expect("write MCP config");

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
        method: "mcpServerStatus/list".to_string(),
        params: Some(json!({
            "limit": 10
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

    let response: ListMcpServerStatusResponse = parse_response(
        outgoing_rx
            .recv()
            .await
            .expect("mcpServerStatus/list extended response envelope"),
        2,
    );
    assert!(response.next_cursor.is_none());
    assert_eq!(response.data.len(), 1);
    let row = &response.data[0];
    assert_eq!(row.name, "disabled_stdio");
    assert!(!row.enabled);
    assert_eq!(row.transport, "echo hello");
    assert_eq!(row.tool_timeout_sec, Some(33.0));
    assert_eq!(row.disabled_tools, vec!["alpha".to_string(), "zeta".to_string()]);
    let startup_timeout = row.startup_timeout_sec.expect("startup timeout should be set");
    assert!((startup_timeout - 12.5).abs() < 0.000_001);
    assert!(row.failure.is_none());
    assert!(row.tools.is_empty());
    assert!(row.resources.is_empty());
    assert!(row.resource_templates.is_empty());
    assert_eq!(
        row.auth_status,
        code_app_server_protocol::McpAuthStatus::Unsupported
    );

    let _ = std::fs::remove_dir_all(temp_code_home);
}

#[tokio::test]
async fn mcp_server_status_list_v2_includes_extended_fields_for_enabled_http_server() {
    let (
        mut processor,
        mut outgoing_rx,
        mut session,
        outbound_initialized,
        outbound_opted_out_notification_methods,
        temp_code_home,
    ) = setup_processor("mcp-status-list-enabled-http");

    let config_path = temp_code_home.join("config.toml");
    std::fs::write(
        &config_path,
        r#"[mcp_servers.enabled_http]
url = "http://127.0.0.1:0"
bearer_token = "token"
startup_timeout_sec = 0.2
tool_timeout_sec = 7
disabled_tools = ["zeta", "alpha", "alpha"]
"#,
    )
    .expect("write MCP config");

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
        method: "mcpServerStatus/list".to_string(),
        params: Some(json!({
            "limit": 10
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

    let response: ListMcpServerStatusResponse = parse_response(
        outgoing_rx
            .recv()
            .await
            .expect("mcpServerStatus/list enabled-http response envelope"),
        2,
    );
    assert!(response.next_cursor.is_none());
    assert_eq!(response.data.len(), 1);
    let row = &response.data[0];
    assert_eq!(row.name, "enabled_http");
    assert!(row.enabled);
    assert_eq!(row.transport, "HTTP http://127.0.0.1:0");
    assert_eq!(row.tool_timeout_sec, Some(7.0));
    assert_eq!(row.disabled_tools, vec!["alpha".to_string(), "zeta".to_string()]);
    let startup_timeout = row.startup_timeout_sec.expect("startup timeout should be set");
    assert!((startup_timeout - 0.2).abs() < 0.000_001);
    let failure = row.failure.as_deref().expect("failure should be set");
    assert!(failure.starts_with("Failed to start:"));
    assert!(row.tools.is_empty());
    assert!(row.resources.is_empty());
    assert!(row.resource_templates.is_empty());
    assert_eq!(
        row.auth_status,
        code_app_server_protocol::McpAuthStatus::BearerToken
    );

    let _ = std::fs::remove_dir_all(temp_code_home);
}
