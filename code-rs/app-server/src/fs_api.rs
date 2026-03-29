use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use code_app_server_protocol::FsCopyParams;
use code_app_server_protocol::FsCopyResponse;
use code_app_server_protocol::FsCreateDirectoryParams;
use code_app_server_protocol::FsCreateDirectoryResponse;
use code_app_server_protocol::FsGetMetadataParams;
use code_app_server_protocol::FsGetMetadataResponse;
use code_app_server_protocol::FsReadDirectoryEntry;
use code_app_server_protocol::FsReadDirectoryParams;
use code_app_server_protocol::FsReadDirectoryResponse;
use code_app_server_protocol::FsReadFileParams;
use code_app_server_protocol::FsReadFileResponse;
use code_app_server_protocol::FsRemoveParams;
use code_app_server_protocol::FsRemoveResponse;
use code_app_server_protocol::FsWriteFileParams;
use code_app_server_protocol::FsWriteFileResponse;
use mcp_types::JSONRPCErrorError;
use tokio::sync::OnceCell;

use crate::error_code::INTERNAL_ERROR_CODE;
use crate::error_code::INVALID_REQUEST_ERROR_CODE;
use crate::exec_server_spawn::SpawnedExecServer;
use crate::exec_server_spawn::resolve_exec_server_binary_path_from_env;
use crate::exec_server_spawn::spawn_exec_server;

#[derive(Clone, Debug)]
enum ExecServerMode {
    Disabled,
    Remote { url: String },
    Spawn,
}

struct ExecServerFsBackend {
    file_system: Arc<dyn code_exec_server::ExecutorFileSystem>,
    _spawned: Option<SpawnedExecServer>,
}

pub(crate) struct FsApi {
    mode: ExecServerMode,
    backend: OnceCell<Result<ExecServerFsBackend, String>>,
}

impl FsApi {
    pub(crate) fn new(config: &code_core::config::Config) -> Self {
        let mode = if let Some(url) = config.experimental_exec_server_url.clone() {
            ExecServerMode::Remote { url }
        } else if config.experimental_spawn_exec_server {
            ExecServerMode::Spawn
        } else {
            ExecServerMode::Disabled
        };

        Self {
            mode,
            backend: OnceCell::new(),
        }
    }

    pub(crate) fn is_enabled(&self) -> bool {
        !matches!(self.mode, ExecServerMode::Disabled)
    }

    async fn backend(&self) -> Result<&ExecServerFsBackend, JSONRPCErrorError> {
        let result = self
            .backend
            .get_or_init(|| async { self.init_backend_with_resolver(resolve_exec_server_binary_path_from_env).await })
            .await;
        match result {
            Ok(backend) => Ok(backend),
            Err(err) => Err(JSONRPCErrorError {
                code: INTERNAL_ERROR_CODE,
                message: err.clone(),
                data: None,
            }),
        }
    }

    async fn init_backend_with_resolver(
        &self,
        resolve_binary: fn() -> Option<std::path::PathBuf>,
    ) -> Result<ExecServerFsBackend, String> {
        match &self.mode {
            ExecServerMode::Disabled => Err("exec-server is disabled".to_string()),
            ExecServerMode::Remote { url } => {
                let env = code_exec_server::Environment::create_with_client_name(
                    Some(url.clone()),
                    "code-app-server".to_string(),
                )
                .await
                .map_err(|err| err.to_string())?;
                Ok(ExecServerFsBackend {
                    file_system: env.get_filesystem(),
                    _spawned: None,
                })
            }
            ExecServerMode::Spawn => {
                let binary = resolve_binary().ok_or_else(|| {
                    "unable to resolve `codex-exec-server` binary path for experimental_spawn_exec_server".to_string()
                })?;
                let spawned = spawn_exec_server(&binary)
                    .await
                    .map_err(|err| format!("failed to spawn `codex-exec-server`: {err}"))?;

                let env = code_exec_server::Environment::create_with_client_name(
                    Some(spawned.listen_url().to_string()),
                    "code-app-server".to_string(),
                )
                .await
                .map_err(|err| format!("failed to connect to spawned exec-server: {err}"))?;

                Ok(ExecServerFsBackend {
                    file_system: env.get_filesystem(),
                    _spawned: Some(spawned),
                })
            }
        }
    }

    pub(crate) async fn read_file(
        &self,
        params: FsReadFileParams,
    ) -> Result<FsReadFileResponse, JSONRPCErrorError> {
        let backend = self.backend().await?;
        let bytes = backend
            .file_system
            .read_file(&params.path)
            .await
            .map_err(map_fs_error)?;
        Ok(FsReadFileResponse {
            data_base64: STANDARD.encode(bytes),
        })
    }

    pub(crate) async fn write_file(
        &self,
        params: FsWriteFileParams,
    ) -> Result<FsWriteFileResponse, JSONRPCErrorError> {
        let bytes = STANDARD.decode(params.data_base64).map_err(|err| JSONRPCErrorError {
            code: INVALID_REQUEST_ERROR_CODE,
            message: format!("fs/writeFile requires valid base64 dataBase64: {err}"),
            data: None,
        })?;
        let backend = self.backend().await?;
        backend
            .file_system
            .write_file(&params.path, bytes)
            .await
            .map_err(map_fs_error)?;
        Ok(FsWriteFileResponse {})
    }

    pub(crate) async fn create_directory(
        &self,
        params: FsCreateDirectoryParams,
    ) -> Result<FsCreateDirectoryResponse, JSONRPCErrorError> {
        let backend = self.backend().await?;
        backend
            .file_system
            .create_directory(
                &params.path,
                code_exec_server::CreateDirectoryOptions {
                    recursive: params.recursive.unwrap_or(true),
                },
            )
            .await
            .map_err(map_fs_error)?;
        Ok(FsCreateDirectoryResponse {})
    }

    pub(crate) async fn get_metadata(
        &self,
        params: FsGetMetadataParams,
    ) -> Result<FsGetMetadataResponse, JSONRPCErrorError> {
        let backend = self.backend().await?;
        let metadata = backend
            .file_system
            .get_metadata(&params.path)
            .await
            .map_err(map_fs_error)?;
        Ok(FsGetMetadataResponse {
            is_directory: metadata.is_directory,
            is_file: metadata.is_file,
            created_at_ms: metadata.created_at_ms,
            modified_at_ms: metadata.modified_at_ms,
        })
    }

    pub(crate) async fn read_directory(
        &self,
        params: FsReadDirectoryParams,
    ) -> Result<FsReadDirectoryResponse, JSONRPCErrorError> {
        let backend = self.backend().await?;
        let entries = backend
            .file_system
            .read_directory(&params.path)
            .await
            .map_err(map_fs_error)?;
        Ok(FsReadDirectoryResponse {
            entries: entries
                .into_iter()
                .map(|entry| FsReadDirectoryEntry {
                    file_name: entry.file_name,
                    is_directory: entry.is_directory,
                    is_file: entry.is_file,
                })
                .collect(),
        })
    }

    pub(crate) async fn remove(
        &self,
        params: FsRemoveParams,
    ) -> Result<FsRemoveResponse, JSONRPCErrorError> {
        let backend = self.backend().await?;
        backend
            .file_system
            .remove(
                &params.path,
                code_exec_server::RemoveOptions {
                    recursive: params.recursive.unwrap_or(true),
                    force: params.force.unwrap_or(true),
                },
            )
            .await
            .map_err(map_fs_error)?;
        Ok(FsRemoveResponse {})
    }

    pub(crate) async fn copy(
        &self,
        params: FsCopyParams,
    ) -> Result<FsCopyResponse, JSONRPCErrorError> {
        let backend = self.backend().await?;
        backend
            .file_system
            .copy(
                &params.source_path,
                &params.destination_path,
                code_exec_server::CopyOptions {
                    recursive: params.recursive,
                },
            )
            .await
            .map_err(map_fs_error)?;
        Ok(FsCopyResponse {})
    }
}

fn map_fs_error(err: std::io::Error) -> JSONRPCErrorError {
    if err.kind() == std::io::ErrorKind::InvalidInput {
        JSONRPCErrorError {
            code: INVALID_REQUEST_ERROR_CODE,
            message: err.to_string(),
            data: None,
        }
    } else {
        JSONRPCErrorError {
            code: INTERNAL_ERROR_CODE,
            message: err.to_string(),
            data: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode_for(url: Option<&str>, spawn: bool) -> ExecServerMode {
        if let Some(url) = url {
            return ExecServerMode::Remote { url: url.to_string() };
        }
        if spawn {
            return ExecServerMode::Spawn;
        }
        ExecServerMode::Disabled
    }

    #[test]
    fn mode_prefers_remote_url_over_spawn() {
        let mode = mode_for(Some("ws://127.0.0.1:9999"), true);
        assert!(matches!(mode, ExecServerMode::Remote { .. }));
    }

    #[tokio::test]
    async fn spawn_mode_errors_when_binary_is_missing() {
        fn resolve_none() -> Option<std::path::PathBuf> {
            None
        }

        let api = FsApi {
            mode: ExecServerMode::Spawn,
            backend: OnceCell::new(),
        };

        let err = match api.init_backend_with_resolver(resolve_none).await {
            Ok(_) => panic!("expected init error"),
            Err(err) => err,
        };
        assert!(err.contains("unable to resolve `codex-exec-server` binary path"));
    }
}
