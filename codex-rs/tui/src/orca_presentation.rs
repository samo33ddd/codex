//! Small semantic previews for the Orca and Codex TUI calls used by the desktop app.

use codex_shell_command::bash::parse_shell_lc_plain_commands;
use codex_shell_command::powershell::extract_powershell_command;
use codex_shell_command::powershell::parse_powershell_command_into_plain_commands;
use codex_shell_command::powershell::parse_powershell_script_into_plain_commands;
use serde_json::Value;

pub(crate) struct ActionPreview {
    pub(crate) action: String,
    pub(crate) detail: Option<String>,
    pub(crate) help: bool,
}

pub(crate) fn safe_output_preview(text: &str) -> Option<String> {
    let text = text.lines().next()?.trim();
    if text.is_empty()
        || text.contains(":\\")
        || text.to_ascii_lowercase().contains(".exe")
        || text.split(|ch: char| !ch.is_ascii_hexdigit() && ch != '-').any(is_uuid)
    {
        return None;
    }
    preview_text(text)
}

fn is_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

pub(crate) fn codex_tui_action(
    server: &str,
    tool: &str,
    arguments: Option<&Value>,
) -> Option<ActionPreview> {
    if !server.eq_ignore_ascii_case("codex_tui") {
        return None;
    }
    let arguments = arguments?.as_object()?;
    match tool {
        "send_message_to_thread" => {
            let message = ["prompt", "message", "text", "content"]
                .into_iter()
                .find_map(|key| arguments.get(key).and_then(Value::as_str))?;
            Some(ActionPreview {
                action: "вызов API: отправка сообщения".to_owned(),
                detail: preview_text(message),
                help: false,
            })
        }
        "set_thread_title" => {
            let title = arguments.get("title").and_then(Value::as_str)?;
            Some(ActionPreview {
                action: "вызов API: смена заголовка треда".to_owned(),
                detail: preview_text(title),
                help: false,
            })
        }
        _ => None,
    }
}

pub(crate) fn orca_cli_action(command: &[String]) -> Option<ActionPreview> {
    let commands = parse_orca_commands(command)?;
    let [tokens] = commands.as_slice() else {
        return None;
    };
    let [executable, group, subcommand, args @ ..] = tokens.as_slice() else {
        return None;
    };
    let executable = executable.rsplit(['\\', '/']).next()?.trim_matches('"');
    if !executable.eq_ignore_ascii_case("orca")
        && !executable.eq_ignore_ascii_case("orca.exe")
    {
        return None;
    }

    let label = match (group.as_str(), subcommand.as_str()) {
        ("orchestration", "worker-start") => "запуск исполнителя",
        ("orchestration", "check") => "проверка оркестрации",
        ("orchestration", "send") => "сообщение координатору",
        ("orchestration", "ask") => "вопрос координатору",
        ("terminal", "list") => "список терминалов",
        ("terminal", "read" | "show" | "status") => "состояние терминала",
        ("terminal", "send" | "write" | "input") => "ввод в терминал",
        ("terminal", "start" | "open" | "attach") => "подключение к терминалу",
        ("terminal", "stop" | "close" | "interrupt") => "остановка терминала",
        _ => return None,
    };
    let help = args.iter().any(|arg| arg == "--help" || arg == "-h");
    let action = if help {
        format!("справка: orca {group} {subcommand}")
    } else {
        format!("команда Orca: {label}")
    };
    let detail = (!help).then(|| match group.as_str() {
        "orchestration" if subcommand == "ask" => flag_value(args, &["--question"]),
        "orchestration" if subcommand == "send" => {
            flag_value(args, &["--subject"]).or_else(|| flag_value(args, &["--body"]))
        }
        "orchestration" if subcommand == "worker-start" => {
            flag_value(args, &["--subject", "--task", "--title"])
        }
        "terminal" => flag_value(args, &["--text", "--input"]),
        _ => None,
    })
    .flatten()
    .and_then(preview_text);
    Some(ActionPreview {
        action,
        detail,
        help,
    })
}

fn parse_orca_commands(command: &[String]) -> Option<Vec<Vec<String>>> {
    if let Some(commands) = parse_shell_lc_plain_commands(command) {
        return Some(commands);
    }
    if let Some((_, script)) = extract_powershell_command(command) {
        if let Some(invocation) = script.trim_start().strip_prefix('&') {
            return parse_powershell_script_into_plain_commands(invocation.trim_start());
        }
        return parse_powershell_command_into_plain_commands(command);
    }
    Some(vec![command.to_vec()])
}

fn flag_value<'a>(args: &'a [String], flags: &[&str]) -> Option<&'a str> {
    for (index, arg) in args.iter().enumerate() {
        if flags.contains(&arg.as_str()) {
            return args.get(index + 1).map(String::as_str);
        }
        if let Some((flag, value)) = arg.split_once('=')
            && flags.contains(&flag)
        {
            return Some(value);
        }
    }
    None
}

fn preview_text(text: &str) -> Option<String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return None;
    }
    let mut chars = text.chars();
    let preview: String = chars.by_ref().take(81).collect();
    Some(if chars.next().is_some() {
        format!("{}…", preview.trim_end())
    } else {
        preview
    })
}

#[cfg(test)]
#[path = "orca_presentation_tests.rs"]
mod tests;
