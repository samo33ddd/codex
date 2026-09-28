//! Grouped command details retain terminal input in raw output without exposing hidden reasoning.

use super::*;
use crate::exec_cell::CommandOutput;
use crate::exec_cell::model::ExecCall;
use crate::history_cell::HistoryCell;
use crate::history_cell::new_reasoning_summary_block;
use crate::history_cell::new_unified_exec_interaction;
use codex_app_server_protocol::CommandExecutionSource;
use pretty_assertions::assert_eq;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

fn completed_read(name: &str, output: &str) -> ExecCell {
    let command = vec!["cat".to_owned(), name.to_owned()];
    let parsed = codex_shell_command::parse_command::parse_command(&command);
    ExecCell::new(
        ExecCall {
            call_id: name.to_owned(),
            command,
            parsed,
            output: Some(CommandOutput::new(/*exit_code*/ 0, output.to_owned())),
            source: CommandExecutionSource::UnifiedExecStartup,
            start_time: None,
            duration: Some(Duration::from_millis(/*millis*/ 5)),
            interaction_input: None,
        },
        /*animations_enabled*/ false,
    )
}

#[test]
fn raw_grouped_history_retains_terminal_input_and_omits_reasoning() {
    let mut group = completed_read("first.txt", "first output");
    group
        .group
        .push_detail(Arc::from(new_reasoning_summary_block(
            vec!["Inspecting the next file".to_owned()],
            Path::new("."),
        ) as Box<dyn HistoryCell>));
    group
        .group
        .push_detail(Arc::new(new_unified_exec_interaction(
            Some("cat first.txt".to_owned()),
            "continue\n".to_owned(),
        )));
    group
        .append_completed(completed_read("second.txt", "second output"))
        .expect("adjacent reads stay grouped");

    let raw = group
        .raw_lines()
        .iter()
        .map(Line::to_string)
        .collect::<Vec<_>>();
    let rich = group
        .transcript_lines(/*width*/ 80)
        .iter()
        .map(Line::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(format!("Rich\n{rich}\n\nRaw\n{}", raw.join("\n")));

    let mut earlier = completed_read("earlier.txt", "earlier output");
    earlier
        .group
        .push_detail(Arc::from(new_reasoning_summary_block(
            vec!["Inspect the first file".to_owned()],
            Path::new("."),
        ) as Box<dyn HistoryCell>));
    earlier
        .group
        .push_detail(Arc::new(new_unified_exec_interaction(
            Some("cat earlier.txt".to_owned()),
            "next\n".to_owned(),
        )));
    let mut expected_raw = earlier.raw_lines();
    expected_raw.push(Line::from(""));
    expected_raw.extend(group.raw_lines());
    let mut expected_rich = earlier.transcript_hyperlink_lines(/*width*/ 80);
    expected_rich.push(HyperlinkLine::from(""));
    expected_rich.extend(group.transcript_hyperlink_lines(/*width*/ 80));
    group.prepend(earlier);
    assert_eq!(group.raw_lines(), expected_raw);
    assert_eq!(
        group.transcript_hyperlink_lines(/*width*/ 80),
        expected_rich
    );
}

#[test]
fn semantic_orca_preview_keeps_raw_command_and_output_reachable() {
    let command = vec![
        r"C:\Program Files\PowerShell\7\pwsh.exe".to_owned(),
        "-NoProfile".to_owned(),
        "-Command".to_owned(),
        r"& 'C:\Users\krach\AppData\Local\Programs\orca\resources\bin\orca.exe' terminal show --terminal 'term_793cb9bd-90ee-4f3b-8661-dcf926f4b937' --json".to_owned(),
    ];
    let output = r#"{"ok":true}"#;
    let cell = ExecCell::new(
        ExecCall {
            call_id: "orca-terminal-show".to_owned(),
            parsed: codex_shell_command::parse_command::parse_command(&command),
            command,
            output: Some(CommandOutput::new(0, output.to_owned())),
            source: CommandExecutionSource::Agent,
            start_time: None,
            duration: Some(Duration::ZERO),
            interaction_input: None,
        },
        /*animations_enabled*/ false,
    );

    let compact = cell
        .compact_hyperlink_lines(/*width*/ 100)
        .iter()
        .map(|line| line.line.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(compact.contains("команда Orca: состояние терминала"));
    assert!(!compact.contains("terminal show"));
    assert!(!compact.contains("{\"ok\":true}"));
    assert!(cell.has_hidden_activity_details(/*width*/ 100));

    let transcript = cell
        .transcript_lines(/*width*/ 200)
        .iter()
        .map(Line::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(transcript.contains("terminal show"));
    assert!(transcript.contains(output));
}
