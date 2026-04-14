//! macOS Data Protection Keychain store.
//!
//! Uses the modern `kSecUseDataProtectionKeychain` API to store credentials
//! in the Secure Enclave-backed keychain.  Items stored here do NOT trigger
//! the legacy "Allow / Deny / Always Allow" Keychain Access dialogs and
//! integrate transparently with Touch ID when configured.
//!
//! Falls back to the legacy `keyring`-crate store on first load if the item
//! doesn't exist in the Data Protection Keychain, enabling transparent
//! migration from older installations.

use security_framework::passwords::{
    delete_generic_password_options, generic_password, set_generic_password_options,
    PasswordOptions,
};
use tracing::trace;

use super::{CredentialStoreError, DefaultKeyringStore, KeyringStore};

/// macOS `errSecItemNotFound` status code (-25300).
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;

/// A [`KeyringStore`] backed by the macOS Data Protection Keychain.
#[derive(Debug)]
pub struct DataProtectionKeyringStore;

impl DataProtectionKeyringStore {
    fn make_options(service: &str, account: &str) -> PasswordOptions {
        let mut opts = PasswordOptions::new_generic_password(service, account);
        opts.use_protected_keychain();
        opts
    }
}

impl KeyringStore for DataProtectionKeyringStore {
    fn load(
        &self,
        service: &str,
        account: &str,
    ) -> Result<Option<String>, CredentialStoreError> {
        trace!("dp-keychain.load start, service={service}, account={account}");

        // Try the Data Protection Keychain first.
        let opts = Self::make_options(service, account);
        match generic_password(opts) {
            Ok(bytes) => {
                trace!("dp-keychain.load success (data-protection), service={service}, account={account}");
                let value = String::from_utf8_lossy(&bytes).into_owned();
                return Ok(Some(value));
            }
            Err(err) if err.code() == ERR_SEC_ITEM_NOT_FOUND => {
                trace!("dp-keychain.load not found in data-protection, trying legacy, service={service}, account={account}");
            }
            Err(err) => {
                trace!("dp-keychain.load error from data-protection, trying legacy, service={service}, account={account}, error={err}");
            }
        }

        // Fall back to the legacy keyring store (login keychain).
        let legacy = DefaultKeyringStore;
        match legacy.load(service, account)? {
            Some(value) => {
                trace!("dp-keychain.load found in legacy, migrating, service={service}, account={account}");
                // Migrate to Data Protection Keychain for future accesses.
                if let Err(err) = self.save(service, account, &value) {
                    trace!("dp-keychain.load migration failed, service={service}, account={account}, error={err}");
                } else if let Err(err) = legacy.delete(service, account) {
                    trace!("dp-keychain.load legacy cleanup failed (non-fatal), service={service}, account={account}, error={err}");
                }
                Ok(Some(value))
            }
            None => {
                trace!("dp-keychain.load no entry anywhere, service={service}, account={account}");
                Ok(None)
            }
        }
    }

    fn save(
        &self,
        service: &str,
        account: &str,
        value: &str,
    ) -> Result<(), CredentialStoreError> {
        trace!(
            "dp-keychain.save start, service={service}, account={account}, value_len={}",
            value.len()
        );
        let opts = Self::make_options(service, account);
        set_generic_password_options(value.as_bytes(), opts).map_err(|err| {
            trace!("dp-keychain.save error, service={service}, account={account}, error={err}");
            CredentialStoreError::new(keyring::Error::PlatformFailure(Box::new(err)))
        })?;
        trace!("dp-keychain.save success, service={service}, account={account}");
        Ok(())
    }

    fn delete(
        &self,
        service: &str,
        account: &str,
    ) -> Result<bool, CredentialStoreError> {
        trace!("dp-keychain.delete start, service={service}, account={account}");
        let opts = Self::make_options(service, account);
        match delete_generic_password_options(opts) {
            Ok(()) => {
                trace!("dp-keychain.delete success, service={service}, account={account}");
                Ok(true)
            }
            Err(err) if err.code() == ERR_SEC_ITEM_NOT_FOUND => {
                trace!("dp-keychain.delete no entry, service={service}, account={account}");
                Ok(false)
            }
            Err(err) => {
                trace!("dp-keychain.delete error, service={service}, account={account}, error={err}");
                Err(CredentialStoreError::new(keyring::Error::PlatformFailure(
                    Box::new(err),
                )))
            }
        }
    }
}
