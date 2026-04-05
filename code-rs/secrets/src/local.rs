use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::atomic::compiler_fence;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use age::decrypt;
use age::encrypt;
use age::scrypt::Identity as ScryptIdentity;
use age::scrypt::Recipient as ScryptRecipient;
use age::secrecy::ExposeSecret;
use age::secrecy::SecretString;
use anyhow::Context;
use anyhow::Result;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use code_keyring_store::KeyringStore;
use rand::TryRngCore;
use rand::rngs::OsRng;
use serde::Deserialize;
use serde::Serialize;
use tracing::warn;

use super::SecretListEntry;
use super::SecretName;
use super::SecretScope;
use super::SecretsBackend;
use super::compute_keyring_account;
use super::keyring_service;

const SECRETS_VERSION: u8 = 1;
const LOCAL_SECRETS_FILENAME: &str = "local.age";
const LOCAL_PASSPHRASE_FILENAME: &str = "passphrase";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct SecretsFile {
    version: u8,
    secrets: BTreeMap<String, String>,
}

impl SecretsFile {
    fn new_empty() -> Self {
        Self {
            version: SECRETS_VERSION,
            secrets: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LocalSecretsBackend {
    code_home: PathBuf,
    keyring_store: Arc<dyn KeyringStore>,
}

impl LocalSecretsBackend {
    pub fn new(code_home: PathBuf, keyring_store: Arc<dyn KeyringStore>) -> Self {
        Self {
            code_home,
            keyring_store,
        }
    }

    pub fn set(&self, scope: &SecretScope, name: &SecretName, value: &str) -> Result<()> {
        anyhow::ensure!(!value.is_empty(), "secret value must not be empty");
        let canonical_key = scope.canonical_key(name);
        let mut file = self.load_file()?;
        file.secrets.insert(canonical_key, value.to_string());
        self.save_file(&file)
    }

    pub fn get(&self, scope: &SecretScope, name: &SecretName) -> Result<Option<String>> {
        let canonical_key = scope.canonical_key(name);
        let file = self.load_file()?;
        Ok(file.secrets.get(&canonical_key).cloned())
    }

    pub fn delete(&self, scope: &SecretScope, name: &SecretName) -> Result<bool> {
        let canonical_key = scope.canonical_key(name);
        let mut file = self.load_file()?;
        let removed = file.secrets.remove(&canonical_key).is_some();
        if removed {
            self.save_file(&file)?;
        }
        Ok(removed)
    }

    pub fn list(&self, scope_filter: Option<&SecretScope>) -> Result<Vec<SecretListEntry>> {
        let file = self.load_file()?;
        let mut entries = Vec::new();
        for canonical_key in file.secrets.keys() {
            let Some(entry) = parse_canonical_key(canonical_key) else {
                warn!("skipping invalid canonical secret key: {canonical_key}");
                continue;
            };
            if let Some(scope) = scope_filter
                && entry.scope != *scope
            {
                continue;
            }
            entries.push(entry);
        }
        Ok(entries)
    }

    fn secrets_dir(&self) -> PathBuf {
        self.code_home.join("secrets")
    }

    fn secrets_path(&self) -> PathBuf {
        self.secrets_dir().join(LOCAL_SECRETS_FILENAME)
    }

    fn passphrase_path(&self) -> PathBuf {
        self.secrets_dir().join(LOCAL_PASSPHRASE_FILENAME)
    }

    fn load_file(&self) -> Result<SecretsFile> {
        let path = self.secrets_path();
        if !path.exists() {
            return Ok(SecretsFile::new_empty());
        }

        let ciphertext = fs::read(&path)
            .with_context(|| format!("failed to read secrets file at {}", path.display()))?;
        let passphrase = self.load_or_create_passphrase()?;
        let plaintext = decrypt_with_passphrase(&ciphertext, &passphrase)?;
        let mut parsed: SecretsFile = serde_json::from_slice(&plaintext).with_context(|| {
            format!(
                "failed to deserialize decrypted secrets file at {}",
                path.display()
            )
        })?;
        if parsed.version == 0 {
            parsed.version = SECRETS_VERSION;
        }
        anyhow::ensure!(
            parsed.version <= SECRETS_VERSION,
            "secrets file version {} is newer than supported version {}",
            parsed.version,
            SECRETS_VERSION
        );
        Ok(parsed)
    }

    fn save_file(&self, file: &SecretsFile) -> Result<()> {
        let dir = self.secrets_dir();
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create secrets dir {}", dir.display()))?;

        let passphrase = self.load_or_create_passphrase()?;
        let plaintext = serde_json::to_vec(file).context("failed to serialize secrets file")?;
        let ciphertext = encrypt_with_passphrase(&plaintext, &passphrase)?;
        let path = self.secrets_path();
        write_file_atomically(&path, &ciphertext)?;
        Ok(())
    }

    fn load_or_create_passphrase(&self) -> Result<SecretString> {
        self.load_or_create_passphrase_impl(cfg!(target_os = "android") || cfg!(test))
    }

    fn load_or_create_passphrase_impl(&self, allow_file_fallback: bool) -> Result<SecretString> {
        let account = compute_keyring_account(&self.code_home);
        let passphrase_path = self.passphrase_path();
        let loaded = self.keyring_store.load(keyring_service(), &account);
        match loaded {
            Ok(Some(existing)) => Ok(SecretString::from(existing)),
            Ok(None) => {
                // Generate a high-entropy key and persist it in the OS keyring.
                // This keeps secrets out of plaintext config while remaining
                // fully local/offline for the MVP.
                let generated = generate_passphrase()?;
                let save = self.keyring_store.save(
                    keyring_service(),
                    &account,
                    generated.expose_secret(),
                );
                match save {
                    Ok(()) => Ok(generated),
                    Err(err) if allow_file_fallback => {
                        warn!(
                            "failed to persist secrets key in keyring; using file fallback: {}",
                            err.message()
                        );
                        // If we already have a passphrase file (previous
                        // Android run), prefer it so we can still decrypt
                        // existing ciphertext.
                        if let Some(existing) = read_passphrase_file(&passphrase_path)? {
                            return Ok(existing);
                        }

                        match write_passphrase_file(&passphrase_path, &generated) {
                            Ok(()) => Ok(generated),
                            Err(err)
                                if err
                                    .downcast_ref::<std::io::Error>()
                                    .is_some_and(|io_err| {
                                        io_err.kind() == std::io::ErrorKind::AlreadyExists
                                    }) =>
                            {
                                read_passphrase_file(&passphrase_path)?
                                    .ok_or_else(|| err)
                            }
                            Err(err) => Err(err),
                        }
                    }
                    Err(err) => Err(anyhow::anyhow!(err.message()))
                        .context("failed to persist secrets key in keyring"),
                }
            }
            Err(err) if allow_file_fallback => {
                warn!(
                    "failed to load secrets key from keyring; using file fallback: {}",
                    err.message()
                );
                if let Some(existing) = read_passphrase_file(&passphrase_path)? {
                    return Ok(existing);
                }
                let generated = generate_passphrase()?;
                write_passphrase_file(&passphrase_path, &generated)?;
                Ok(generated)
            }
            Err(err) => Err(anyhow::anyhow!(err.message()))
                .with_context(|| format!("failed to load secrets key from keyring for {account}")),
        }
    }
}

impl SecretsBackend for LocalSecretsBackend {
    fn set(&self, scope: &SecretScope, name: &SecretName, value: &str) -> Result<()> {
        LocalSecretsBackend::set(self, scope, name, value)
    }

    fn get(&self, scope: &SecretScope, name: &SecretName) -> Result<Option<String>> {
        LocalSecretsBackend::get(self, scope, name)
    }

    fn delete(&self, scope: &SecretScope, name: &SecretName) -> Result<bool> {
        LocalSecretsBackend::delete(self, scope, name)
    }

    fn list(&self, scope_filter: Option<&SecretScope>) -> Result<Vec<SecretListEntry>> {
        LocalSecretsBackend::list(self, scope_filter)
    }
}

fn write_file_atomically(path: &Path, contents: &[u8]) -> Result<()> {
    let dir = path.parent().with_context(|| {
        format!(
            "failed to compute parent directory for secrets file at {}",
            path.display()
        )
    })?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let tmp_path = dir.join(format!(
        ".{LOCAL_SECRETS_FILENAME}.tmp-{}-{nonce}",
        std::process::id()
    ));

    {
        let mut tmp_file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&tmp_path)
            .with_context(|| {
                format!(
                    "failed to create temp secrets file at {}",
                    tmp_path.display()
                )
            })?;
        tmp_file.write_all(contents).with_context(|| {
            format!(
                "failed to write temp secrets file at {}",
                tmp_path.display()
            )
        })?;
        tmp_file.sync_all().with_context(|| {
            format!("failed to sync temp secrets file at {}", tmp_path.display())
        })?;
    }

    match fs::rename(&tmp_path, path) {
        Ok(()) => Ok(()),
        Err(initial_error) => {
            #[cfg(target_os = "windows")]
            {
                if path.exists() {
                    fs::remove_file(path).with_context(|| {
                        format!(
                            "failed to remove existing secrets file at {} before replace",
                            path.display()
                        )
                    })?;
                    fs::rename(&tmp_path, path).with_context(|| {
                        format!(
                            "failed to replace secrets file at {} with {}",
                            path.display(),
                            tmp_path.display()
                        )
                    })?;
                    return Ok(());
                }
            }

            let _ = fs::remove_file(&tmp_path);
            Err(initial_error).with_context(|| {
                format!(
                    "failed to atomically replace secrets file at {} with {}",
                    path.display(),
                    tmp_path.display()
                )
            })
        }
    }
}

fn generate_passphrase() -> Result<SecretString> {
    let mut bytes = [0_u8; 32];
    let mut rng = OsRng;
    rng.try_fill_bytes(&mut bytes)
        .context("failed to generate random secrets key")?;
    // Base64 keeps the keyring payload ASCII-safe without reducing entropy.
    let encoded = BASE64_STANDARD.encode(bytes);
    wipe_bytes(&mut bytes);
    Ok(SecretString::from(encoded))
}

fn wipe_bytes(bytes: &mut [u8]) {
    for byte in bytes {
        // Volatile writes make it much harder for the compiler to elide the wipe.
        // SAFETY: `byte` is a valid mutable reference into `bytes`.
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
    compiler_fence(Ordering::SeqCst);
}

fn read_passphrase_file(path: &Path) -> Result<Option<SecretString>> {
    match fs::read_to_string(path) {
        Ok(contents) => {
            let trimmed = contents.trim_end_matches(&['\r', '\n'][..]);
            anyhow::ensure!(
                !trimmed.trim().is_empty(),
                "secrets passphrase file is empty at {}",
                path.display()
            );
            Ok(Some(SecretString::from(trimmed.to_string())))
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(err).with_context(|| {
            format!(
                "failed to read secrets passphrase file at {}",
                path.display()
            )
        }),
    }
}

fn write_passphrase_file(path: &Path, passphrase: &SecretString) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| {
            format!(
                "failed to create secrets directory for passphrase file at {}",
                dir.display()
            )
        })?;
    }

    let mut options = fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }

    match options.open(path) {
        Ok(mut file) => {
            file.write_all(passphrase.expose_secret().as_bytes())
                .with_context(|| format!("failed to write secrets passphrase file at {}", path.display()))?;
            file.write_all(b"\n")
                .with_context(|| format!("failed to finish secrets passphrase file at {}", path.display()))?;
            Ok(())
        }
        Err(err) => Err(err).with_context(|| {
            format!(
                "failed to create secrets passphrase file at {}",
                path.display()
            )
        }),
    }
}

fn encrypt_with_passphrase(plaintext: &[u8], passphrase: &SecretString) -> Result<Vec<u8>> {
    let recipient = ScryptRecipient::new(passphrase.clone());
    encrypt(&recipient, plaintext).context("failed to encrypt secrets file")
}

fn decrypt_with_passphrase(ciphertext: &[u8], passphrase: &SecretString) -> Result<Vec<u8>> {
    let identity = ScryptIdentity::new(passphrase.clone());
    decrypt(&identity, ciphertext).context("failed to decrypt secrets file")
}

fn parse_canonical_key(canonical_key: &str) -> Option<SecretListEntry> {
    let mut parts = canonical_key.split('/');
    let scope_kind = parts.next()?;
    match scope_kind {
        "global" => {
            let name = parts.next()?;
            if parts.next().is_some() {
                return None;
            }
            let name = SecretName::new(name).ok()?;
            Some(SecretListEntry {
                scope: SecretScope::Global,
                name,
            })
        }
        "env" => {
            let environment_id = parts.next()?;
            let name = parts.next()?;
            if parts.next().is_some() {
                return None;
            }
            let name = SecretName::new(name).ok()?;
            let scope = SecretScope::environment(environment_id.to_string()).ok()?;
            Some(SecretListEntry { scope, name })
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use code_keyring_store::tests::MockKeyringStore;
    use keyring::Error as KeyringError;
    use std::io::ErrorKind;

    #[test]
    fn load_file_rejects_newer_schema_versions() -> Result<()> {
        let code_home = tempfile::tempdir().expect("tempdir");
        let keyring = Arc::new(MockKeyringStore::default());
        let backend = LocalSecretsBackend::new(code_home.path().to_path_buf(), keyring);

        let file = SecretsFile {
            version: SECRETS_VERSION + 1,
            secrets: BTreeMap::new(),
        };
        backend.save_file(&file)?;

        let error = backend
            .load_file()
            .expect_err("must reject newer schema version");
        assert!(
            error.to_string().contains("newer than supported version"),
            "unexpected error: {error:#}"
        );
        Ok(())
    }

    #[test]
    fn load_or_create_passphrase_fails_when_keyring_is_unavailable_and_fallback_disabled()
        -> Result<()>
    {
        let code_home = tempfile::tempdir().expect("tempdir");
        let keyring = Arc::new(MockKeyringStore::default());
        let account = compute_keyring_account(code_home.path());
        keyring.set_error(
            &account,
            KeyringError::Invalid("error".into(), "load".into()),
        );

        let backend = LocalSecretsBackend::new(code_home.path().to_path_buf(), keyring);
        let error = backend
            .load_or_create_passphrase_impl(false)
            .expect_err("must fail when keyring load fails and fallback is disabled");
        assert!(
            error
                .to_string()
                .contains("failed to load secrets key from keyring"),
            "unexpected error: {error:#}"
        );
        Ok(())
    }

    #[test]
    fn set_succeeds_with_file_fallback_when_keyring_is_unavailable() -> Result<()> {
        let code_home = tempfile::tempdir().expect("tempdir");
        let keyring = Arc::new(MockKeyringStore::default());
        let account = compute_keyring_account(code_home.path());
        keyring.set_error(
            &account,
            KeyringError::Invalid("error".into(), "load".into()),
        );

        let backend = LocalSecretsBackend::new(code_home.path().to_path_buf(), keyring);
        let scope = SecretScope::Global;
        let name = SecretName::new("TEST_SECRET")?;
        backend.set(&scope, &name, "secret-value")?;
        assert_eq!(backend.get(&scope, &name)?, Some("secret-value".to_string()));
        assert!(backend.passphrase_path().exists());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;

            let mode = fs::metadata(backend.passphrase_path())?.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        Ok(())
    }

    #[test]
    fn keyring_save_failure_uses_existing_passphrase_file() -> Result<()> {
        let code_home = tempfile::tempdir().expect("tempdir");
        let keyring = Arc::new(MockKeyringStore::default());
        let account = compute_keyring_account(code_home.path());
        keyring.set_error(
            &account,
            KeyringError::Invalid("error".into(), "save".into()),
        );

        let backend = LocalSecretsBackend::new(code_home.path().to_path_buf(), keyring);
        let passphrase_path = backend.passphrase_path();

        // Seed a passphrase file to simulate a previous Android run where the
        // keyring save failed.
        let seeded = SecretString::from("seeded-passphrase".to_string());
        write_passphrase_file(&passphrase_path, &seeded)?;

        let resolved = backend.load_or_create_passphrase_impl(true)?;
        assert_eq!(resolved.expose_secret(), seeded.expose_secret());

        // Ensure we didn't overwrite the file.
        let reloaded = read_passphrase_file(&passphrase_path)?
            .expect("passphrase file should still exist");
        assert_eq!(reloaded.expose_secret(), seeded.expose_secret());

        Ok(())
    }

    #[test]
    fn write_passphrase_file_errors_on_existing_file() -> Result<()> {
        let code_home = tempfile::tempdir().expect("tempdir");
        let keyring = Arc::new(MockKeyringStore::default());
        let backend = LocalSecretsBackend::new(code_home.path().to_path_buf(), keyring);
        let passphrase_path = backend.passphrase_path();

        let first = SecretString::from("one".to_string());
        write_passphrase_file(&passphrase_path, &first)?;

        let second = SecretString::from("two".to_string());
        let err = write_passphrase_file(&passphrase_path, &second)
            .expect_err("should refuse to overwrite existing passphrase file");
        let io_err = err.downcast_ref::<std::io::Error>().expect("io error");
        assert_eq!(io_err.kind(), ErrorKind::AlreadyExists);

        Ok(())
    }

    #[test]
    fn save_file_does_not_leave_temp_files() -> Result<()> {
        let code_home = tempfile::tempdir().expect("tempdir");
        let keyring = Arc::new(MockKeyringStore::default());
        let backend = LocalSecretsBackend::new(code_home.path().to_path_buf(), keyring);

        let scope = SecretScope::Global;
        let name = SecretName::new("TEST_SECRET")?;
        backend.set(&scope, &name, "one")?;
        backend.set(&scope, &name, "two")?;

        let secrets_dir = backend.secrets_dir();
        let entries = fs::read_dir(&secrets_dir)
            .with_context(|| format!("failed to read {}", secrets_dir.display()))?
            .collect::<std::io::Result<Vec<_>>>()
            .with_context(|| format!("failed to enumerate {}", secrets_dir.display()))?;

        let filenames: Vec<String> = entries
            .into_iter()
            .filter_map(|entry| entry.file_name().to_str().map(ToString::to_string))
            .collect();
        assert_eq!(filenames, vec![LOCAL_SECRETS_FILENAME.to_string()]);
        assert_eq!(backend.get(&scope, &name)?, Some("two".to_string()));
        Ok(())
    }
}
