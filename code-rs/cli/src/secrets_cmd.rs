use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use anyhow::Result;
use code_keyring_store::KeyringStore;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Global,
    Env,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeSelection {
    pub kind: ScopeKind,
    pub cwd: Option<PathBuf>,
    pub env_id: Option<String>,
}

impl ScopeSelection {
    pub fn to_scope(&self) -> Result<code_secrets::SecretScope> {
        match self.kind {
            ScopeKind::Global => Ok(code_secrets::SecretScope::Global),
            ScopeKind::Env => {
                let env_id = if let Some(env_id) = self.env_id.as_deref() {
                    env_id.trim().to_string()
                } else {
                    let cwd = self.cwd.clone().unwrap_or_else(|| {
                        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
                    });
                    code_secrets::environment_id_from_cwd(&cwd)
                };
                anyhow::ensure!(!env_id.is_empty(), "environment id must not be empty");
                Ok(code_secrets::SecretScope::Environment(env_id))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretsCommand {
    Set {
        name: String,
        value: String,
        scope: ScopeSelection,
    },
    Get {
        name: String,
        reveal: bool,
        scope: ScopeSelection,
    },
    List {
        scope: Option<ScopeSelection>,
    },
    Delete {
        name: String,
        scope: ScopeSelection,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecretsCommandOutput {
    pub exit_code: i32,
    pub stdout: String,
}

pub fn read_secret_value_from_reader(mut reader: impl Read) -> Result<String> {
    let mut buf = String::new();
    reader
        .read_to_string(&mut buf)
        .context("failed to read secret value from stdin")?;
    if buf.ends_with('\n') {
        buf.pop();
        if buf.ends_with('\r') {
            buf.pop();
        }
    }
    anyhow::ensure!(!buf.is_empty(), "secret value must not be empty");
    Ok(buf)
}

pub fn run_secrets_command(
    keyring_store: Arc<dyn KeyringStore>,
    code_home: PathBuf,
    command: SecretsCommand,
) -> Result<SecretsCommandOutput> {
    let manager = code_secrets::SecretsManager::new_with_keyring_store(
        code_home,
        code_secrets::SecretsBackendKind::Local,
        keyring_store,
    );

    match command {
        SecretsCommand::Set { name, value, scope } => {
            let scope = scope.to_scope()?;
            let name = code_secrets::SecretName::new(&name)?;
            manager.set(&scope, &name, &value)?;
            Ok(SecretsCommandOutput {
                exit_code: 0,
                stdout: "saved\n".to_string(),
            })
        }
        SecretsCommand::Get {
            name,
            reveal,
            scope,
        } => {
            let scope = scope.to_scope()?;
            let name = code_secrets::SecretName::new(&name)?;
            let value = manager.get(&scope, &name)?;
            match (reveal, value) {
                (true, Some(value)) => Ok(SecretsCommandOutput {
                    exit_code: 0,
                    stdout: format!("{value}\n"),
                }),
                (false, Some(_)) => Ok(SecretsCommandOutput {
                    exit_code: 0,
                    stdout: "found\n".to_string(),
                }),
                (_, None) => Ok(SecretsCommandOutput {
                    exit_code: 1,
                    stdout: "missing\n".to_string(),
                }),
            }
        }
        SecretsCommand::List { scope } => {
            let scope_filter = match &scope {
                Some(selection) => Some(selection.to_scope()?),
                None => None,
            };
            let listed = manager.list(scope_filter.as_ref())?;
            let mut lines = Vec::new();
            match scope_filter {
                Some(_) => {
                    for entry in listed {
                        lines.push(entry.name.as_str().to_string());
                    }
                }
                None => {
                    for entry in listed {
                        lines.push(entry.scope.canonical_key(&entry.name));
                    }
                }
            }

            let mut stdout = lines.join("\n");
            stdout.push('\n');
            Ok(SecretsCommandOutput {
                exit_code: 0,
                stdout,
            })
        }
        SecretsCommand::Delete { name, scope } => {
            let scope = scope.to_scope()?;
            let name = code_secrets::SecretName::new(&name)?;
            let removed = manager.delete(&scope, &name)?;
            Ok(SecretsCommandOutput {
                exit_code: 0,
                stdout: if removed {
                    "deleted\n".to_string()
                } else {
                    "not-found\n".to_string()
                },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_keyring_store::tests::MockKeyringStore;
    use pretty_assertions::assert_eq;
    use std::io::Cursor;

    #[test]
    fn stdin_reader_trims_single_trailing_newline() -> Result<()> {
        assert_eq!(
            read_secret_value_from_reader(Cursor::new("hello\n"))?,
            "hello"
        );
        assert_eq!(
            read_secret_value_from_reader(Cursor::new("hello\r\n"))?,
            "hello"
        );
        assert_eq!(
            read_secret_value_from_reader(Cursor::new("hello\n\n"))?,
            "hello\n"
        );
        Ok(())
    }

    #[test]
    fn stdin_reader_rejects_empty_values() {
        let err = read_secret_value_from_reader(Cursor::new(""))
            .expect_err("empty stdin must be rejected");
        assert!(err.to_string().contains("secret value must not be empty"));
        let err = read_secret_value_from_reader(Cursor::new("\n"))
            .expect_err("newline-only stdin must be rejected");
        assert!(err.to_string().contains("secret value must not be empty"));
    }

    #[test]
    fn get_default_hides_value_and_sets_exit_code() -> Result<()> {
        let code_home = tempfile::tempdir().expect("tempdir");
        let store = Arc::new(MockKeyringStore::default());
        let scope = ScopeSelection {
            kind: ScopeKind::Global,
            cwd: None,
            env_id: None,
        };

        let out = run_secrets_command(
            Arc::clone(&store) as Arc<dyn KeyringStore>,
            code_home.path().to_path_buf(),
            SecretsCommand::Get {
                name: "OPENAI_API_KEY".to_string(),
                reveal: false,
                scope: scope.clone(),
            },
        )?;
        assert_eq!(out.exit_code, 1);
        assert_eq!(out.stdout, "missing\n");

        let out = run_secrets_command(
            Arc::clone(&store) as Arc<dyn KeyringStore>,
            code_home.path().to_path_buf(),
            SecretsCommand::Set {
                name: "OPENAI_API_KEY".to_string(),
                value: "sk-test".to_string(),
                scope: scope.clone(),
            },
        )?;
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "saved\n");

        let out = run_secrets_command(
            Arc::clone(&store) as Arc<dyn KeyringStore>,
            code_home.path().to_path_buf(),
            SecretsCommand::Get {
                name: "OPENAI_API_KEY".to_string(),
                reveal: false,
                scope: scope.clone(),
            },
        )?;
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "found\n");

        let out = run_secrets_command(
            Arc::clone(&store) as Arc<dyn KeyringStore>,
            code_home.path().to_path_buf(),
            SecretsCommand::Get {
                name: "OPENAI_API_KEY".to_string(),
                reveal: true,
                scope,
            },
        )?;
        assert_eq!(out.exit_code, 0);
        assert_eq!(out.stdout, "sk-test\n");
        Ok(())
    }
}
