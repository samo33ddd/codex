use super::*;
use crate::legacy_core::config::ConfigBuilder;

#[tokio::test]
async fn owner_is_forwarded_for_start_fork_and_both_resume_shapes() {
    let temp_dir = tempfile::tempdir().expect("tempdir");
    let config = ConfigBuilder::default()
        .codex_home(temp_dir.path().to_path_buf())
        .build()
        .await
        .expect("config should build");
    let thread_id = ThreadId::new();
    let owner = HookOwner {
        pane_key: "00000000-0000-4000-8000-000000000001".to_string(),
        tab_id: Some("00000000-0000-4000-8000-000000000002".to_string()),
        worktree_id: Some("00000000-0000-4000-8000-000000000003".to_string()),
        launch_token: Some("00000000-0000-4000-8000-000000000004".to_string()),
        hook_endpoint_path: std::env::temp_dir()
            .join("orca-hook-endpoint.json")
            .display()
            .to_string(),
    };

    let start = thread_start_params_from_config(
        &config,
        ThreadParamsMode::Embedded,
        /*remote_cwd_override*/ None,
        /*session_start_source*/ None,
        Some(owner.clone()),
    );
    let fork = thread_fork_params_from_config(
        config.clone(),
        thread_id,
        ThreadParamsMode::Embedded,
        /*remote_cwd_override*/ None,
        Some(owner.clone()),
    );
    let resume = thread_resume_params_from_config(
        config.clone(),
        thread_id,
        ThreadParamsMode::Embedded,
        /*remote_cwd_override*/ None,
        ResumeModelSettings::RestoreFromThread,
        Some(owner.clone()),
    );
    let preserved_resume = thread_resume_params_from_config(
        config.clone(),
        thread_id,
        ThreadParamsMode::Embedded,
        /*remote_cwd_override*/ None,
        ResumeModelSettings::PreserveExistingThread,
        Some(owner.clone()),
    );

    assert_eq!(start.hook_owner, Some(owner.clone()));
    assert_eq!(fork.hook_owner, Some(owner.clone()));
    assert_eq!(resume.hook_owner, Some(owner.clone()));
    assert_eq!(preserved_resume.hook_owner, Some(owner.clone()));

    let remote_start = thread_start_params_from_config(
        &config,
        ThreadParamsMode::Remote,
        /*remote_cwd_override*/ None,
        /*session_start_source*/ None,
        Some(owner.clone()),
    );
    let remote_resume = thread_resume_params_from_config(
        config.clone(),
        thread_id,
        ThreadParamsMode::Remote,
        /*remote_cwd_override*/ None,
        ResumeModelSettings::PreserveExistingThread,
        Some(owner.clone()),
    );
    let remote_fork = thread_fork_params_from_config(
        config,
        thread_id,
        ThreadParamsMode::Remote,
        /*remote_cwd_override*/ None,
        Some(owner),
    );
    assert_eq!(remote_start.hook_owner, None);
    assert_eq!(remote_resume.hook_owner, None);
    assert_eq!(remote_fork.hook_owner, None);
}
