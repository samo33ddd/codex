use super::Target;
use super::capture_for_target;
use std::collections::HashMap;

const OWNER_ENV: [&str; 5] = [
    "ORCA_PANE_KEY",
    "ORCA_TAB_ID",
    "ORCA_WORKTREE_ID",
    "ORCA_AGENT_LAUNCH_TOKEN",
    "ORCA_AGENT_HOOK_ENDPOINT",
];

fn owner_environment() -> HashMap<&'static str, String> {
    HashMap::from([
        (
            OWNER_ENV[0],
            "00000000-0000-4000-8000-000000000001".to_string(),
        ),
        (
            OWNER_ENV[1],
            "00000000-0000-4000-8000-000000000002".to_string(),
        ),
        (
            OWNER_ENV[2],
            "00000000-0000-4000-8000-000000000003".to_string(),
        ),
        (
            OWNER_ENV[3],
            "00000000-0000-4000-8000-000000000004".to_string(),
        ),
        (
            OWNER_ENV[4],
            std::env::temp_dir()
                .join("orca-hook-endpoint.json")
                .display()
                .to_string(),
        ),
    ])
}

#[test]
fn local_daemon_without_hook_owner_support_fails_only_when_orca_identity_is_present() {
    let values = owner_environment();
    let error = capture_for_target(Target::LocalDaemonUnsupported, |name| {
        Ok(values.get(name).cloned())
    })
    .expect_err("an old local daemon must reject Orca-owned sessions");
    assert!(error.contains("does not support hook ownership"));

    assert_eq!(
        capture_for_target(Target::LocalDaemonUnsupported, |_| Ok(None))
            .expect("ordinary CLI starts remain compatible"),
        None
    );
}

#[test]
fn local_daemon_capture_reads_only_the_fixed_allowlist() {
    let mut values = owner_environment();
    values.insert("ORCA_AGENT_HOOK_TOKEN", "must-not-be-read".to_string());
    values.insert("OPENAI_API_KEY", "must-not-be-read".to_string());
    let mut reads = Vec::new();
    let owner = capture_for_target(Target::LocalDaemonSupported, |name| {
        reads.push(name.to_string());
        Ok(values.get(name).cloned())
    })
    .expect("supported local daemon accepts the owner")
    .expect("Orca identity values were supplied");

    assert_eq!(reads, OWNER_ENV.map(str::to_string).to_vec());
    assert_eq!(owner.pane_key, "00000000-0000-4000-8000-000000000001");
    assert_eq!(
        owner.tab_id.as_deref(),
        Some("00000000-0000-4000-8000-000000000002")
    );
    assert_eq!(
        owner.worktree_id.as_deref(),
        Some("00000000-0000-4000-8000-000000000003")
    );
    assert_eq!(
        owner.launch_token.as_deref(),
        Some("00000000-0000-4000-8000-000000000004")
    );
    assert_eq!(owner.hook_endpoint_path, values[OWNER_ENV[4]]);
}

#[test]
fn remote_targets_do_not_read_or_send_local_owner_values() {
    let owner = capture_for_target(Target::Remote, |_| {
        panic!("remote targets must not inspect local Orca environment values")
    })
    .expect("remote target omits local ownership");

    assert_eq!(owner, None);
}
