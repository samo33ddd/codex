use std::fmt;
use std::path::Path;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use ts_rs::TS;

const MAX_SERIALIZED_BYTES: usize = 32 * 1024;
const MAX_PANE_OR_TAB_UTF16_UNITS: usize = 200;
const MAX_WORKTREE_OR_TOKEN_BYTES: usize = 1024;
const MAX_ENDPOINT_PATH_BYTES: usize = 16 * 1024;

/// The fixed Orca pane identity that may be sent over authenticated local daemon IPC.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(rename_all = "camelCase", export_to = "protocol/")]
pub struct HookOwner {
    pub pane_key: String,
    pub tab_id: Option<String>,
    pub worktree_id: Option<String>,
    pub launch_token: Option<String>,
    pub hook_endpoint_path: String,
}

impl fmt::Debug for HookOwner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HookOwner")
            .field("redacted", &true)
            .finish()
    }
}

impl HookOwner {
    /// Reject malformed values before they cross the daemon boundary.
    pub fn validate(&self) -> Result<(), &'static str> {
        let values = [
            Some(self.pane_key.as_str()),
            self.tab_id.as_deref(),
            self.worktree_id.as_deref(),
            self.launch_token.as_deref(),
            Some(self.hook_endpoint_path.as_str()),
        ];
        if values
            .into_iter()
            .flatten()
            .any(|value| value.bytes().any(|byte| matches!(byte, 0 | b'\n' | b'\r')))
        {
            return Err("hookOwner values must not contain NUL or newline characters");
        }
        if self.pane_key.is_empty() || self.hook_endpoint_path.is_empty() {
            return Err("hookOwner.paneKey and hookEndpointPath are required");
        }
        if self.pane_key.encode_utf16().count() > MAX_PANE_OR_TAB_UTF16_UNITS
            || self
                .tab_id
                .as_ref()
                .is_some_and(|value| value.encode_utf16().count() > MAX_PANE_OR_TAB_UTF16_UNITS)
        {
            return Err("hookOwner paneKey and tabId are limited to 200 UTF-16 units");
        }
        if self
            .worktree_id
            .as_ref()
            .is_some_and(|value| value.len() > MAX_WORKTREE_OR_TOKEN_BYTES)
            || self
                .launch_token
                .as_ref()
                .is_some_and(|value| value.len() > MAX_WORKTREE_OR_TOKEN_BYTES)
        {
            return Err("hookOwner worktreeId and launchToken are limited to 1024 bytes");
        }
        if self.hook_endpoint_path.len() > MAX_ENDPOINT_PATH_BYTES {
            return Err("hookOwner.hookEndpointPath is limited to 16384 bytes");
        }
        if !Path::new(&self.hook_endpoint_path).is_absolute() {
            return Err("hookOwner.hookEndpointPath must be absolute");
        }
        if serde_json::to_vec(self).map_or(true, |value| value.len() > MAX_SERIALIZED_BYTES) {
            return Err("hookOwner exceeds the 32 KiB size limit");
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "hook_owner_tests.rs"]
mod tests;
