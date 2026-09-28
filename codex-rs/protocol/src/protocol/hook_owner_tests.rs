use super::HookOwner;

#[test]
fn validation_accepts_fixed_local_owner_and_rejects_malformed_values() {
    let valid = HookOwner {
        pane_key: "pane-1".to_string(),
        tab_id: Some("tab-1".to_string()),
        worktree_id: Some("worktree-1".to_string()),
        launch_token: Some("sentinel-launch-token".to_string()),
        hook_endpoint_path: if cfg!(windows) {
            "C:\\orca\\hook-endpoint.json".to_string()
        } else {
            "/tmp/orca/hook-endpoint.json".to_string()
        },
    };
    assert_eq!(valid.validate(), Ok(()));
    assert!(!format!("{valid:?}").contains("sentinel-launch-token"));

    let mut malformed = valid.clone();
    malformed.pane_key = "bad\nvalue".to_string();
    assert_eq!(
        malformed.validate(),
        Err("hookOwner values must not contain NUL or newline characters")
    );

    let mut relative_endpoint = valid.clone();
    relative_endpoint.hook_endpoint_path = "hook-endpoint.json".to_string();
    assert_eq!(
        relative_endpoint.validate(),
        Err("hookOwner.hookEndpointPath must be absolute")
    );

    let mut oversized = valid;
    oversized.pane_key = "x".repeat(201);
    assert_eq!(
        oversized.validate(),
        Err("hookOwner paneKey and tabId are limited to 200 UTF-16 units")
    );
}

#[test]
fn validation_enforces_owner_field_limits_and_required_values() {
    let mut owner = HookOwner {
        pane_key: "p".repeat(200),
        tab_id: Some("t".repeat(200)),
        worktree_id: Some("w".repeat(1024)),
        launch_token: Some("l".repeat(1024)),
        hook_endpoint_path: if cfg!(windows) {
            format!("C:\\{}", "e".repeat(16_381))
        } else {
            format!("/{}", "e".repeat(16_383))
        },
    };
    assert_eq!(owner.validate(), Ok(()));

    let mut unicode = owner.clone();
    unicode.pane_key = "😀".repeat(100);
    assert_eq!(unicode.validate(), Ok(()));
    unicode.pane_key = "😀".repeat(101);
    assert_eq!(
        unicode.validate(),
        Err("hookOwner paneKey and tabId are limited to 200 UTF-16 units")
    );

    owner.tab_id = Some("t".repeat(201));
    assert_eq!(
        owner.validate(),
        Err("hookOwner paneKey and tabId are limited to 200 UTF-16 units")
    );

    owner.tab_id = None;
    owner.worktree_id = Some("w".repeat(1025));
    assert_eq!(
        owner.validate(),
        Err("hookOwner worktreeId and launchToken are limited to 1024 bytes")
    );

    owner.worktree_id = None;
    owner.launch_token = Some("l".repeat(1025));
    assert_eq!(
        owner.validate(),
        Err("hookOwner worktreeId and launchToken are limited to 1024 bytes")
    );

    owner.launch_token = None;
    owner.hook_endpoint_path.push('e');
    assert_eq!(
        owner.validate(),
        Err("hookOwner.hookEndpointPath is limited to 16384 bytes")
    );

    owner.hook_endpoint_path = if cfg!(windows) {
        "C:\\orca\\endpoint.json".to_string()
    } else {
        "/tmp/orca/endpoint.json".to_string()
    };
    owner.pane_key.clear();
    assert_eq!(
        owner.validate(),
        Err("hookOwner.paneKey and hookEndpointPath are required")
    );
}
