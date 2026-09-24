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
        r"C:\Program Files\PowerShell\7\pwsh.exe".to_owned(),
        "-NoProfile".to_owned(),
        "-Command".to_owned(),
        r"& 'C:\Users\krach\AppData\Local\Programs\orca\resources\bin\orca.exe' terminal show --terminal 'term_793cb9bd-90ee-4f3b-8661-dcf926f4b937' --json".to_owned(),
    ];
    let powershell_summary =
        orca_cli_action(&powershell).expect("quoted Windows terminal command");
    assert_eq!(
        powershell_summary.action,
        "команда Orca: состояние терминала"
    );
    assert!(!powershell_summary.help);
}

#[test]
fn orca_orchestration_commands_have_readable_labels() {
    for (subcommand, label) in [
        ("run-create", "создание запуска оркестрации"),
        ("task-create", "создание задачи"),
        ("task-list", "список задач"),
        ("task-update", "обновление задачи"),
        ("worker-read", "чтение вывода исполнителя"),
        ("worker-show", "сведения об исполнителе"),
        ("worker-list", "список исполнителей"),
        ("worker-release", "освобождение терминала исполнителя"),
        ("worker-stop", "остановка исполнителя"),
    ] {
        let command = vec![
            "orca.exe".to_owned(),
            "orchestration".to_owned(),
            subcommand.to_owned(),
        ];
        let preview = orca_cli_action(&command).expect("known orchestration command");
        assert_eq!(preview.action, format!("команда Orca: {label}"));
    }
}

#[test]
fn quoted_orca_send_with_literal_here_string_has_a_safe_preview() {
    let powershell = vec![
        r"C:\Program Files\PowerShell\7\pwsh.exe".to_owned(),
        "-NoProfile".to_owned(),
        "-Command".to_owned(),
        r#"& 'C:\Users\krach\AppData\Local\Programs\orca\resources\bin\orca.exe' orchestration send --subject 'TUI check complete' --body @'
{"taskId":"task_50e598b53214","detail":"raw JSON must stay in details"}
'@ --json"#
            .to_owned(),
    ];
    let preview = orca_cli_action(&powershell).expect("quoted Orca orchestration send");
    assert_eq!(preview.action, "команда Orca: сообщение координатору");
    assert_eq!(preview.detail.as_deref(), Some("TUI check complete"));
    assert!(!preview.detail.unwrap_or_default().contains('{'));
}

#[test]
fn orca_powershell_dynamic_and_compound_commands_fall_back() {
    let dynamic = vec![
        r"C:\Program Files\PowerShell\7\pwsh.exe".to_owned(),
        "-NoProfile".to_owned(),
        "-Command".to_owned(),
        r"& 'C:\Users\krach\AppData\Local\Programs\orca\resources\bin\orca.exe' orchestration send --body $message".to_owned(),
    ];
    assert!(orca_cli_action(&dynamic).is_none());

    let compound = vec![
        r"C:\Program Files\PowerShell\7\pwsh.exe".to_owned(),
        "-NoProfile".to_owned(),
        "-Command".to_owned(),
        r"& 'C:\Users\krach\AppData\Local\Programs\orca\resources\bin\orca.exe' terminal show --json; Write-Output done".to_owned(),
    ];
    assert!(orca_cli_action(&compound).is_none());
}

#[test]
fn tavily_search_and_extract_have_readable_safe_previews() {
    let search = codex_tui_action(
        "codex_apps.tavily",
        "tavily_search",
        Some(&json!({"query": "Rust tree-sitter PowerShell literal invocation"})),
    )
    .expect("Tavily search preview");
    assert_eq!(search.action, "веб-поиск");
    assert_eq!(
        search.detail.as_deref(),
        Some("Rust tree-sitter PowerShell literal invocation")
    );

    let extract = codex_tui_action(
        "codex_apps.tavily",
        "tavily_extract",
        Some(&json!({"urls": ["https://example.com/first", "https://example.com/second"]})),
    )
    .expect("Tavily extraction preview");
    assert_eq!(extract.action, "извлечение страниц");
    assert_eq!(
        extract.detail.as_deref(),
        Some("https://example.com/first, https://example.com/second")
    );
    assert!(!extract.detail.unwrap_or_default().contains('{'));
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
