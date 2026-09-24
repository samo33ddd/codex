use super::ThreadParamsMode;
use codex_app_server_client::AppServerClient;
use codex_protocol::protocol::HookOwner;

#[derive(Clone, Copy)]
enum Target {
    Remote,
    LocalDaemonSupported,
    LocalDaemonUnsupported,
}

pub(super) fn capture(
    client: &AppServerClient,
    thread_params_mode: ThreadParamsMode,
) -> std::result::Result<Option<HookOwner>, &'static str> {
    let target = match (thread_params_mode, client) {
        (ThreadParamsMode::Embedded, AppServerClient::Remote(_))
            if client.supports_hook_owner() =>
        {
            Target::LocalDaemonSupported
        }
        (ThreadParamsMode::Embedded, AppServerClient::Remote(_)) => Target::LocalDaemonUnsupported,
        _ => Target::Remote,
    };
    capture_for_target(target, |name| match std::env::var(name) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err("Orca hook-owner identity values must be valid UTF-8")
        }
    })
}

fn capture_for_target(
    target: Target,
    mut read: impl FnMut(&str) -> std::result::Result<Option<String>, &'static str>,
) -> std::result::Result<Option<HookOwner>, &'static str> {
    if matches!(target, Target::Remote) {
        return Ok(None);
    }

    let values = [
        read("ORCA_PANE_KEY")?,
        read("ORCA_TAB_ID")?,
        read("ORCA_WORKTREE_ID")?,
        read("ORCA_AGENT_LAUNCH_TOKEN")?,
        read("ORCA_AGENT_HOOK_ENDPOINT")?,
    ];
    if values.iter().all(Option::is_none) {
        return Ok(None);
    }

    if matches!(target, Target::LocalDaemonUnsupported) {
        return Err(
            "this Orca local daemon does not support hook ownership; restart it with hook-owner support",
        );
    }

    let [
        pane_key,
        tab_id,
        worktree_id,
        launch_token,
        hook_endpoint_path,
    ] = values;
    let owner = HookOwner {
        pane_key: pane_key.ok_or("ORCA_PANE_KEY is required for Orca hook ownership")?,
        tab_id,
        worktree_id,
        launch_token,
        hook_endpoint_path: hook_endpoint_path
            .ok_or("ORCA_AGENT_HOOK_ENDPOINT is required for Orca hook ownership")?,
    };
    owner.validate()?;
    Ok(Some(owner))
}

#[cfg(test)]
#[path = "hook_owner_tests.rs"]
mod tests;
