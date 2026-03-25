use crate::models::FileSystemPermissions;
use crate::models::NetworkPermissions;
use crate::models::PermissionProfile;
use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(rename_all = "snake_case")]
pub enum PermissionGrantScope {
    #[default]
    Turn,
    Session,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq, JsonSchema, TS)]
#[serde(deny_unknown_fields)]
pub struct RequestPermissionProfile {
    pub network: Option<NetworkPermissions>,
    pub file_system: Option<FileSystemPermissions>,
}

impl RequestPermissionProfile {
    pub fn is_empty(&self) -> bool {
        let network_enabled = self
            .network
            .as_ref()
            .and_then(|permissions| permissions.enabled)
            .unwrap_or(false);

        !network_enabled
            && self
                .file_system
                .as_ref()
                .map(FileSystemPermissions::is_empty)
                .unwrap_or(true)
    }
}

impl From<RequestPermissionProfile> for PermissionProfile {
    fn from(value: RequestPermissionProfile) -> Self {
        Self {
            // `NetworkPermissions { enabled: None }` is treated as empty for our internal bool representation.
            network: value
                .network
                .and_then(|permissions| permissions.enabled)
                .filter(|enabled| *enabled),
            file_system: value.file_system,
            macos: None,
        }
    }
}

impl From<PermissionProfile> for RequestPermissionProfile {
    fn from(value: PermissionProfile) -> Self {
        Self {
            network: value.network.filter(|enabled| *enabled).map(|enabled| NetworkPermissions {
                enabled: Some(enabled),
            }),
            file_system: value.file_system,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema, TS)]
pub struct RequestPermissionsArgs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub permissions: RequestPermissionProfile,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema, TS)]
pub struct RequestPermissionsResponse {
    pub permissions: RequestPermissionProfile,
    #[serde(default)]
    pub scope: PermissionGrantScope,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, JsonSchema, TS)]
pub struct RequestPermissionsEvent {
    /// Responses API call id for the associated tool call, if available.
    pub call_id: String,
    /// Turn ID that this request belongs to.
    /// Uses `#[serde(default)]` for backwards compatibility.
    #[serde(default)]
    pub turn_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    pub permissions: RequestPermissionProfile,
}
