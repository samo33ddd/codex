use super::codex_tui_action;
use super::orca_cli_action;
use super::safe_output_preview;
use serde_json::json;

#[test]
fn codex_tui_prompt_is_the_send_message_preview() {
    let summary = codex_tui_action(
        "codex_tui",
        "send_message_to_thread",
        Some(&json!({"threadId": "025e4ed2-490f-49ef-bfac-814625506917", "prompt": "hello from the prompt"})),
    )
    .expect("known Codex TUI action");
    assert_eq!(summary.detail.as_deref(), Some("hello from the prompt"));
}

#[test]
fn orca_help_and_quoted_windows_invocations_are_classified_conservatively() {
    let direct = vec![
        r"C:\Program Files\Orca\orca.exe".to_owned(),
        "orchestration".to_owned(),
        "worker-start".to_owned(),
        "--help".to_owned(),
    ];
    let direct_summary = orca_cli_action(&direct).expect("known direct Orca command");
    assert!(direct_summary.help);
    assert!(direct_summary.action.starts_with("справка:"));
    assert!(!direct_summary.action.contains("запуск исполнителя"));

    let powershell = vec![
        "pwsh".to_owned(),
        "-NoProfile".to_owned(),
        "-Command".to_owned(),
        "& 'C:\\Program Files\\Orca\\orca.exe' orchestration worker-start --help".to_owned(),
    ];
    assert!(orca_cli_action(&powershell).is_some_and(|summary| summary.help));
}

#[test]
fn compound_and_unknown_commands_fall_back() {
    let compound = vec![
        "bash".to_owned(),
        "-lc".to_owned(),
        "orca orchestration worker-start --help && echo done".to_owned(),
    ];
    assert!(orca_cli_action(&compound).is_none());
    assert!(
        orca_cli_action(&[
            "other-tool".to_owned(),
            "orchestration".to_owned(),
            "check".to_owned()
        ])
        .is_none()
    );
}

#[test]
fn safe_output_preview_skips_json_structure_lines() {
    assert_eq!(safe_output_preview("{\n  \"ok\": true\n}"), None);
    assert_eq!(safe_output_preview(" [\n  1\n]"), None);
    assert_eq!(
        safe_output_preview("Status: ready"),
        Some("Status: ready".to_owned())
    );
}
