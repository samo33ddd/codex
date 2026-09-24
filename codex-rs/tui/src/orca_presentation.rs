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

const POWERSHELL_EXECUTABLE_PLACEHOLDER: &str = "__orca_literal_executable__";
const POWERSHELL_HERE_STRING_PLACEHOLDER: &str = "__orca_literal_here_string__";

pub(crate) fn safe_output_preview(text: &str) -> Option<String> {
    let text = text.lines().next()?.trim();
    if text.is_empty()
        || text.starts_with('{')
        || text.starts_with('[')
        || text.contains(":\\")
        || text.to_ascii_lowercase().contains(".exe")
        || text
            .split(|ch: char| !ch.is_ascii_hexdigit() && ch != '-')
            .any(is_uuid)
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

pub(crate) fn mcp_action_preview(
    server: &str,
    tool: &str,
    arguments: Option<&Value>,
) -> Option<ActionPreview> {
    let arguments = arguments?.as_object()?;
    if server.eq_ignore_ascii_case("codex_tui") {
        return match tool {
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
        };
    }

    if !server.eq_ignore_ascii_case("codex_apps") {
        return None;
    }
    match tool {
        "tavily.tavily_search" => {
            let query = arguments.get("query").and_then(Value::as_str)?;
            Some(ActionPreview {
                action: "веб-поиск".to_owned(),
                detail: preview_text(query),
                help: false,
            })
        }
        "tavily.tavily_extract" => {
            let urls = arguments
                .get("urls")?
                .as_array()?
                .iter()
                .filter_map(Value::as_str)
                .filter(|url| !url.is_empty())
                .collect::<Vec<_>>()
                .join(", ");
            if urls.is_empty() {
                return None;
            }
            let detail = match arguments
                .get("query")
                .and_then(Value::as_str)
                .filter(|query| !query.is_empty())
            {
                Some(query) => format!("{query} · {urls}"),
                None => urls,
            };
            Some(ActionPreview {
                action: "извлечение страниц".to_owned(),
                detail: preview_text(&detail),
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
    if !executable.eq_ignore_ascii_case("orca") && !executable.eq_ignore_ascii_case("orca.exe") {
        return None;
    }

    let label = match (group.as_str(), subcommand.as_str()) {
        ("orchestration", "worker-start") => "запуск исполнителя",
        ("orchestration", "run-create") => "создание запуска оркестрации",
        ("orchestration", "task-create") => "создание задачи",
        ("orchestration", "task-list") => "список задач",
        ("orchestration", "task-update") => "обновление задачи",
        ("orchestration", "check") => "проверка оркестрации",
        ("orchestration", "send") => "сообщение координатору",
        ("orchestration", "ask") => "вопрос координатору",
        ("orchestration", "worker-read") => "чтение вывода исполнителя",
        ("orchestration", "worker-show") => "сведения об исполнителе",
        ("orchestration", "worker-list") => "список исполнителей",
        ("orchestration", "worker-release") => "освобождение терминала исполнителя",
        ("orchestration", "worker-stop") => "остановка исполнителя",
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
    let detail = (!help)
        .then(|| match group.as_str() {
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
        .filter(|detail| *detail != POWERSHELL_HERE_STRING_PLACEHOLDER)
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
        if script.trim_start().starts_with('&') {
            return parse_powershell_call_operator_invocation(script);
        }
        return parse_powershell_command_into_plain_commands(command);
    }
    Some(vec![command.to_vec()])
}

fn parse_powershell_call_operator_invocation(script: &str) -> Option<Vec<Vec<String>>> {
    let invocation = script.trim_start().strip_prefix('&')?;
    if !invocation.chars().next().is_some_and(char::is_whitespace) {
        return None;
    }
    let executable = invocation.trim_start().strip_prefix('\'')?;
    let quote_end = executable.find('\'')?;
    if quote_end == 0 || executable.as_bytes().get(quote_end + 1) == Some(&b'\'') {
        return None;
    }
    let executable_path = &executable[..quote_end];
    if executable_path.contains(['\r', '\n']) {
        return None;
    }
    let arguments = &executable[quote_end + 1..];
    if !arguments.is_empty() && !arguments.chars().next().is_some_and(char::is_whitespace) {
        return None;
    }

    let script = format!("{POWERSHELL_EXECUTABLE_PLACEHOLDER}{arguments}");
    let script = mask_literal_powershell_here_string(&script)?;
    let mut commands = parse_powershell_script_into_plain_commands(&script)?;
    let executable_token = commands.first_mut()?.first_mut()?;
    if executable_token != POWERSHELL_EXECUTABLE_PLACEHOLDER {
        return None;
    }
    *executable_token = executable_path.to_owned();
    Some(commands)
}

fn mask_literal_powershell_here_string(script: &str) -> Option<String> {
    let start = script.match_indices("@'").find_map(|(index, _)| {
        (index == 0
            || script[..index]
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace))
        .then_some(index)
    });
    let Some(start) = start else {
        return Some(script.to_owned());
    };
    let prefix = &script[..start];
    if prefix.contains('#') {
        return None;
    }
    let prefix_commands = parse_powershell_script_into_plain_commands(prefix)?;
    let [prefix_tokens] = prefix_commands.as_slice() else {
        return None;
    };
    if prefix_tokens.first()?.as_str() != POWERSHELL_EXECUTABLE_PLACEHOLDER {
        return None;
    }
    let after_open = start + "@'".len();
    let body_start = if script[after_open..].starts_with("\r\n") {
        after_open + "\r\n".len()
    } else if script[after_open..].starts_with('\n') {
        after_open + '\n'.len_utf8()
    } else {
        return None;
    };
    let close_start = body_start + script[body_start..].find("\n'@")?;
    let close_end = close_start + "\n'@".len();
    let trailing = &script[close_end..];
    if !trailing.is_empty() && !trailing.chars().next().is_some_and(char::is_whitespace) {
        return None;
    }
    Some(format!(
        "{}{POWERSHELL_HERE_STRING_PLACEHOLDER}{trailing}",
        &script[..start]
    ))
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
