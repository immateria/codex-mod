use std::collections::VecDeque;
use std::io::Write;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::process::Child;
use tokio::process::ChildStdin;
use tokio::process::ChildStdout;
use tokio::process::Command;

use code_app_server_protocol::ClientInfo;
use code_app_server_protocol::ClientRequest;
use code_app_server_protocol::CommandExecutionApprovalDecision;
use code_app_server_protocol::CommandExecutionRequestApprovalResponse;
use code_app_server_protocol::ExecCommandApprovalResponse;
use code_app_server_protocol::FileChangeApprovalDecision;
use code_app_server_protocol::FileChangeRequestApprovalResponse;
use code_app_server_protocol::JSONRPCError;
use code_app_server_protocol::JSONRPCErrorError;
use code_app_server_protocol::JSONRPCMessage;
use code_app_server_protocol::JSONRPCNotification;
use code_app_server_protocol::JSONRPCRequest;
use code_app_server_protocol::JSONRPCResponse;
use code_app_server_protocol::RequestId;
use code_app_server_protocol::ServerNotification;
use code_app_server_protocol::ServerRequest;
use code_app_server_protocol::ThreadStartParams;
use code_app_server_protocol::ThreadStartResponse;
use code_app_server_protocol::ToolRequestUserInputResponse;
use code_app_server_protocol::TurnStartParams;
use code_app_server_protocol::TurnStartResponse;
use code_app_server_protocol::TurnStatus;
use code_app_server_protocol::UserInput as V2UserInput;
use code_app_server_protocol::ApplyPatchApprovalResponse;
use code_app_server_protocol::DynamicToolCallOutputContentItem;
use code_app_server_protocol::DynamicToolCallResponse;
use code_protocol::protocol::ReviewDecision;

pub async fn send_message_v2(
    code_bin: &Path,
    config_overrides: &[String],
    user_message: String,
) -> Result<()> {
    let mut client = AppServerClient::spawn_stdio(code_bin, config_overrides).await?;
    client.initialize().await?;

    let thread_response = client.thread_start(ThreadStartParams::default()).await?;
    let turn_response = client
        .turn_start(TurnStartParams {
            thread_id: thread_response.thread.id.clone(),
            input: vec![V2UserInput::Text {
                text: user_message,
                text_elements: Vec::new(),
            }],
            ..Default::default()
        })
        .await?;

    client
        .stream_turn(&thread_response.thread.id, &turn_response.turn.id)
        .await?;

    client.shutdown().await?;
    Ok(())
}

struct AppServerClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: tokio::io::Lines<BufReader<ChildStdout>>,
    pending_notifications: VecDeque<JSONRPCNotification>,
    next_request_id: i64,
}

impl AppServerClient {
    async fn spawn_stdio(code_bin: &Path, config_overrides: &[String]) -> Result<Self> {
        let code_bin_display = code_bin.display();
        let mut cmd = Command::new(code_bin);
        for override_kv in config_overrides {
            cmd.arg("--config").arg(override_kv);
        }

        let mut child = cmd
            .arg("app-server")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("failed to start `{code_bin_display}` app-server"))?;

        let stdin = child.stdin.take().context("app-server stdin unavailable")?;
        let stdout = child.stdout.take().context("app-server stdout unavailable")?;
        Ok(Self {
            child,
            stdin: Some(stdin),
            stdout: BufReader::new(stdout).lines(),
            pending_notifications: VecDeque::new(),
            next_request_id: 1,
        })
    }

    async fn shutdown(&mut self) -> Result<()> {
        // Closing stdin triggers the server to shutdown in stdio mode.
        drop(self.stdin.take());

        let status = tokio::time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .context("timed out waiting for app-server to exit")?
            .context("failed waiting for app-server to exit")?;
        if !status.success() {
            anyhow::bail!("app-server exited with status {status}");
        }
        Ok(())
    }

    fn request_id(&mut self) -> RequestId {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.saturating_add(1);
        RequestId::Integer(id)
    }

    async fn initialize(&mut self) -> Result<()> {
        let request_id = self.request_id();
        let request = ClientRequest::Initialize {
            request_id: request_id.clone(),
            params: code_app_server_protocol::InitializeParams {
                client_info: ClientInfo {
                    name: "code-cli-debug".to_string(),
                    title: Some("Code CLI Debug".to_string()),
                    version: env!("CARGO_PKG_VERSION").to_string(),
                },
                capabilities: Some(code_app_server_protocol::InitializeCapabilities {
                    experimental_api: true,
                }),
            },
        };

        let _resp: code_app_server_protocol::InitializeResponse =
            self.send_request(request, request_id, "initialize").await?;

        // Optional handshake marker used by some clients; safe to send even if ignored.
        self.write_jsonrpc_message(JSONRPCMessage::Notification(JSONRPCNotification {
            method: "initialized".to_string(),
            params: None,
        }))
        .await?;

        Ok(())
    }

    async fn thread_start(&mut self, params: ThreadStartParams) -> Result<ThreadStartResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::ThreadStart {
            request_id: request_id.clone(),
            params,
        };
        self.send_request(request, request_id, "thread/start").await
    }

    async fn turn_start(&mut self, params: TurnStartParams) -> Result<TurnStartResponse> {
        let request_id = self.request_id();
        let request = ClientRequest::TurnStart {
            request_id: request_id.clone(),
            params,
        };
        self.send_request(request, request_id, "turn/start").await
    }

    async fn stream_turn(&mut self, thread_id: &str, turn_id: &str) -> Result<()> {
        loop {
            let notification = self.next_notification().await?;
            let Ok(server_notification) = ServerNotification::try_from(notification) else {
                continue;
            };

            match server_notification {
                ServerNotification::AgentMessageDelta(delta) => {
                    print!("{}", delta.delta);
                    std::io::stdout().flush().ok();
                }
                ServerNotification::CommandExecutionOutputDelta(delta) => {
                    print!("{}", delta.delta);
                    std::io::stdout().flush().ok();
                }
                ServerNotification::TurnCompleted(payload) => {
                    if payload.thread_id == thread_id && payload.turn.id == turn_id {
                        if payload.turn.status != TurnStatus::Completed {
                            eprintln!(
                                "\n[turn completed: {status:?}]",
                                status = payload.turn.status
                            );
                        } else {
                            // Ensure the final assistant output ends with a newline.
                            println!();
                        }
                        break;
                    }
                }
                _ => {}
            }
        }

        Ok(())
    }

    async fn send_request<T>(
        &mut self,
        request: ClientRequest,
        request_id: RequestId,
        method: &str,
    ) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        self.write_request(&request).await?;
        self.wait_for_response(request_id, method).await
    }

    async fn write_request(&mut self, request: &ClientRequest) -> Result<()> {
        let payload = serde_json::to_string(request).context("failed to serialize request")?;
        self.write_payload(&payload).await
    }

    async fn wait_for_response<T>(&mut self, request_id: RequestId, method: &str) -> Result<T>
    where
        T: serde::de::DeserializeOwned,
    {
        loop {
            let message = self.read_jsonrpc_message().await?;
            match message {
                JSONRPCMessage::Response(JSONRPCResponse { id, result }) => {
                    if id == request_id {
                        return serde_json::from_value(result).with_context(|| {
                            format!("{method} response was not the expected shape")
                        });
                    }
                }
                JSONRPCMessage::Error(err) => {
                    if err.id == request_id {
                        anyhow::bail!("{method} failed: {err:?}");
                    }
                }
                JSONRPCMessage::Notification(notification) => {
                    self.pending_notifications.push_back(notification);
                }
                JSONRPCMessage::Request(request) => {
                    self.handle_server_request(request).await?;
                }
            }
        }
    }

    async fn next_notification(&mut self) -> Result<JSONRPCNotification> {
        if let Some(notification) = self.pending_notifications.pop_front() {
            return Ok(notification);
        }

        loop {
            match self.read_jsonrpc_message().await? {
                JSONRPCMessage::Notification(notification) => return Ok(notification),
                JSONRPCMessage::Response(_) | JSONRPCMessage::Error(_) => continue,
                JSONRPCMessage::Request(request) => {
                    self.handle_server_request(request).await?;
                }
            }
        }
    }

    async fn read_jsonrpc_message(&mut self) -> Result<JSONRPCMessage> {
        loop {
            let Some(line) = self
                .stdout
                .next_line()
                .await
                .context("failed to read from app-server")?
            else {
                anyhow::bail!("app-server closed stdout");
            };

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let mut value: serde_json::Value =
                serde_json::from_str(trimmed).context("app-server output was not valid JSON")?;
            if let serde_json::Value::Object(map) = &mut value {
                map.remove("jsonrpc");
            }

            let message: JSONRPCMessage = serde_json::from_value(value)
                .context("app-server output was not a valid JSON-RPC message")?;
            return Ok(message);
        }
    }

    async fn handle_server_request(&mut self, request: JSONRPCRequest) -> Result<()> {
        let request_id = request.id.clone();
        let server_request = ServerRequest::try_from(request)
            .context("failed to decode ServerRequest from JSON-RPC request")?;

        match server_request {
            ServerRequest::CommandExecutionRequestApproval { request_id, .. } => {
                let response = CommandExecutionRequestApprovalResponse {
                    decision: CommandExecutionApprovalDecision::Decline,
                };
                self.send_server_request_response(request_id, &response).await?;
            }
            ServerRequest::FileChangeRequestApproval { request_id, .. } => {
                let response = FileChangeRequestApprovalResponse {
                    decision: FileChangeApprovalDecision::Decline,
                };
                self.send_server_request_response(request_id, &response).await?;
            }
            ServerRequest::ApplyPatchApproval { request_id, .. } => {
                let response = ApplyPatchApprovalResponse {
                    decision: ReviewDecision::Denied,
                };
                self.send_server_request_response(request_id, &response).await?;
            }
            ServerRequest::ExecCommandApproval { request_id, .. } => {
                let response = ExecCommandApprovalResponse {
                    decision: ReviewDecision::Denied,
                };
                self.send_server_request_response(request_id, &response).await?;
            }
            ServerRequest::ToolRequestUserInput { request_id, .. } => {
                let response = ToolRequestUserInputResponse {
                    answers: std::collections::HashMap::new(),
                };
                self.send_server_request_response(request_id, &response).await?;
            }
            ServerRequest::DynamicToolCall { request_id, params } => {
                let response = DynamicToolCallResponse {
                    content_items: vec![DynamicToolCallOutputContentItem::InputText {
                        text: format!(
                            "dynamic tool `{tool}` not supported by this debug client",
                            tool = params.tool
                        ),
                    }],
                    success: false,
                };
                self.send_server_request_response(request_id, &response).await?;
            }
            other => {
                self.send_server_request_error(
                    request_id,
                    -32601,
                    format!("unsupported server request: {other:?}"),
                )
                .await?;
            }
        }

        Ok(())
    }

    async fn send_server_request_response<T>(&mut self, request_id: RequestId, response: &T) -> Result<()>
    where
        T: serde::Serialize,
    {
        let message = JSONRPCMessage::Response(JSONRPCResponse {
            id: request_id,
            result: serde_json::to_value(response).context("failed to serialize response")?,
        });
        self.write_jsonrpc_message(message).await
    }

    async fn send_server_request_error(
        &mut self,
        request_id: RequestId,
        code: i64,
        message: String,
    ) -> Result<()> {
        self.write_jsonrpc_message(JSONRPCMessage::Error(JSONRPCError {
            id: request_id,
            error: JSONRPCErrorError {
                code,
                message,
                data: None,
            },
        }))
        .await
    }

    async fn write_jsonrpc_message(&mut self, message: JSONRPCMessage) -> Result<()> {
        let payload = serde_json::to_string(&message).context("failed to serialize JSON-RPC")?;
        self.write_payload(&payload).await
    }

    async fn write_payload(&mut self, payload: &str) -> Result<()> {
        let Some(stdin) = self.stdin.as_mut() else {
            anyhow::bail!("app-server stdin closed");
        };

        stdin
            .write_all(payload.as_bytes())
            .await
            .context("failed to write payload to app-server")?;
        stdin
            .write_all(b"\n")
            .await
            .context("failed to write newline to app-server")?;
        stdin.flush().await.context("failed to flush app-server stdin")?;
        Ok(())
    }
}
