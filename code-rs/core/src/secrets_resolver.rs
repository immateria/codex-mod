use std::path::Path;

/// Where a secret was resolved from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecretValueSource {
    EnvVar,
    SecretsEnvScope,
    SecretsGlobal,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedSecret {
    pub value: String,
    pub source: SecretValueSource,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SecretLookupOutcome {
    pub resolved: Option<ResolvedSecret>,
    pub error: Option<String>,
}

pub fn resolve_secret_env_or_store(
    name: &str,
    cwd: &Path,
    secrets: Option<&code_secrets::SecretsManager>,
) -> SecretLookupOutcome {
    if let Ok(value) = std::env::var(name)
        && !value.trim().is_empty()
    {
        return SecretLookupOutcome {
            resolved: Some(ResolvedSecret {
                value,
                source: SecretValueSource::EnvVar,
            }),
            error: None,
        };
    }

    let Some(secrets) = secrets else {
        return SecretLookupOutcome::default();
    };

    let secret_name = match code_secrets::SecretName::new(name) {
        Ok(name) => name,
        Err(err) => {
            return SecretLookupOutcome {
                resolved: None,
                error: Some(err.to_string()),
            };
        }
    };

    let env_scope =
        code_secrets::SecretScope::Environment(code_secrets::environment_id_from_cwd(cwd));
    match secrets.get(&env_scope, &secret_name) {
        Ok(Some(value)) if !value.trim().is_empty() => {
            return SecretLookupOutcome {
                resolved: Some(ResolvedSecret {
                    value,
                    source: SecretValueSource::SecretsEnvScope,
                }),
                error: None,
            };
        }
        Ok(_) => {}
        Err(err) => {
            return SecretLookupOutcome {
                resolved: None,
                error: Some(format!(
                    "failed to read secrets store for {name} (env scope): {err}"
                )),
            };
        }
    }

    match secrets.get(&code_secrets::SecretScope::Global, &secret_name) {
        Ok(Some(value)) if !value.trim().is_empty() => SecretLookupOutcome {
            resolved: Some(ResolvedSecret {
                value,
                source: SecretValueSource::SecretsGlobal,
            }),
            error: None,
        },
        Ok(_) => SecretLookupOutcome::default(),
        Err(err) => SecretLookupOutcome {
            resolved: None,
            error: Some(format!(
                "failed to read secrets store for {name} (global scope): {err}"
            )),
        },
    }
}

pub fn resolve_secret_env_or_store_for_code_home(
    name: &str,
    code_home: &Path,
    cwd: &Path,
) -> SecretLookupOutcome {
    let secrets = code_secrets::SecretsManager::new(
        code_home.to_path_buf(),
        code_secrets::SecretsBackendKind::Local,
    );
    resolve_secret_env_or_store(name, cwd, Some(&secrets))
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_keyring_store::tests::MockKeyringStore;
    use std::sync::Arc;

    struct EnvVarGuard {
        key: &'static str,
        prev: Option<String>,
    }

    impl EnvVarGuard {
        fn new(key: &'static str) -> Self {
            Self {
                key,
                prev: std::env::var(key).ok(),
            }
        }
    }

    impl Drop for EnvVarGuard {
        fn drop(&mut self) {
            match self.prev.as_ref() {
                Some(value) => unsafe { std::env::set_var(self.key, value) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    #[test]
    fn env_var_wins_over_secrets() -> anyhow::Result<()> {
        let _guard = EnvVarGuard::new("GITHUB_TOKEN");
        unsafe { std::env::set_var("GITHUB_TOKEN", "env-token") };

        let code_home = tempfile::tempdir().expect("tempdir");
        let keyring = Arc::new(MockKeyringStore::default());
        let secrets = code_secrets::SecretsManager::new_with_keyring_store(
            code_home.path().to_path_buf(),
            code_secrets::SecretsBackendKind::Local,
            keyring,
        );
        secrets.set(
            &code_secrets::SecretScope::Global,
            &code_secrets::SecretName::new("GITHUB_TOKEN")?,
            "secrets-token",
        )?;

        let outcome = resolve_secret_env_or_store(
            "GITHUB_TOKEN",
            &std::env::current_dir().expect("cwd"),
            Some(&secrets),
        );
        assert_eq!(
            outcome.resolved,
            Some(ResolvedSecret {
                value: "env-token".to_string(),
                source: SecretValueSource::EnvVar,
            })
        );
        Ok(())
    }

    #[test]
    fn secrets_env_scope_precedes_global() -> anyhow::Result<()> {
        let _guard = EnvVarGuard::new("OPENAI_API_KEY");
        unsafe { std::env::remove_var("OPENAI_API_KEY") };

        let code_home = tempfile::tempdir().expect("tempdir");
        let keyring = Arc::new(MockKeyringStore::default());
        let secrets = code_secrets::SecretsManager::new_with_keyring_store(
            code_home.path().to_path_buf(),
            code_secrets::SecretsBackendKind::Local,
            keyring,
        );

        let name = code_secrets::SecretName::new("OPENAI_API_KEY")?;
        secrets.set(&code_secrets::SecretScope::Global, &name, "global-token")?;

        let cwd = tempfile::tempdir().expect("cwd");
        let env_scope = code_secrets::SecretScope::Environment(code_secrets::environment_id_from_cwd(cwd.path()));
        secrets.set(&env_scope, &name, "env-scope-token")?;

        let outcome = resolve_secret_env_or_store("OPENAI_API_KEY", cwd.path(), Some(&secrets));
        assert_eq!(
            outcome.resolved,
            Some(ResolvedSecret {
                value: "env-scope-token".to_string(),
                source: SecretValueSource::SecretsEnvScope,
            })
        );
        Ok(())
    }
}

