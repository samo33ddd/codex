//! Conservative action summaries for literal PowerShell command bodies.

use std::collections::HashSet;
use std::path::PathBuf;

use codex_protocol::parse_command::ParsedCommand;

use crate::command_safety::try_parse_powershell_commands_with_source;

pub(crate) fn parse_powershell_script(script: &str) -> Vec<ParsedCommand> {
    let unknown = || {
        vec![ParsedCommand::Unknown {
            cmd: script.to_owned(),
        }]
    };
    let Some(commands) = try_parse_powershell_commands_with_source(script) else {
        return unknown();
    };
    let mut actions = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        let Some(action) = classify(&command.words, &command.quoted, script) else {
            // No stage of a chain may disappear from its presentation by accident.
            return unknown();
        };
        if let Some(action) = action {
            actions.push(action);
        } else if index != 1
            || commands.len() != 2
            || script[commands[0].range.end..command.range.start].trim() != "|"
        {
            // A selector is meaningful only as a filter on an earlier action.
            return unknown();
        }
    }
    if actions.is_empty() {
        unknown()
    } else {
        actions
    }
}

// Some(read/search/list) is an action; Some(None) is a known, non-mutating selector.
fn classify(words: &[String], quoted: &[bool], script: &str) -> Option<Option<ParsedCommand>> {
    let (head, args) = words.split_first()?;
    let quoted = &quoted[1..];
    let name = head.to_ascii_lowercase();
    let action = match name.as_str() {
        "get-content" | "gc" | "type" | "cat" => {
            let path = cmdlet_path(
                args,
                quoted,
                &["raw"],
                &["totalcount", "head", "first", "tail", "last", "readcount"],
                &["encoding"],
            )?;
            ParsedCommand::Read {
                cmd: script.to_owned(),
                name: basename(&path),
                path: PathBuf::from(path.replace('\\', "/")),
            }
        }
        "get-childitem" | "gci" | "dir" | "ls" => {
            let (path, filter) = listing_args(args, quoted)?;
            // The output protocol has no separate listing filter; a filtered listing can be
            // represented as a search only when it has a concrete pattern.
            if let Some(query) = filter {
                ParsedCommand::Search {
                    cmd: script.to_owned(),
                    query: Some(query),
                    path: path.as_deref().map(basename),
                }
            } else {
                ParsedCommand::ListFiles {
                    cmd: script.to_owned(),
                    path: path.as_deref().map(basename),
                }
            }
        }
        "select-string" | "sls" => {
            let (query, path) = select_string_args(args, quoted)?;
            ParsedCommand::Search {
                cmd: script.to_owned(),
                query: Some(query),
                path: path.as_deref().map(basename),
            }
        }
        "rg" | "rg.exe" | "rga" | "rga.exe" | "ripgrep-all" | "ripgrep-all.exe" => {
            let (files, query, path) = rg_args(args)?;
            if files {
                ParsedCommand::ListFiles {
                    cmd: script.to_owned(),
                    path: path.as_deref().map(basename),
                }
            } else {
                ParsedCommand::Search {
                    cmd: script.to_owned(),
                    query,
                    path: path.as_deref().map(basename),
                }
            }
        }
        "git" | "git.exe" => git_action(args, script)?,
        "bat" | "bat.exe" | "batcat" | "batcat.exe" | "less" | "less.exe" | "more.com" | "head"
        | "head.exe" | "tail" | "tail.exe" => {
            let path = native_reader_path(&name, args)?;
            ParsedCommand::Read {
                cmd: script.to_owned(),
                name: basename(&path),
                path: PathBuf::from(path.replace('\\', "/")),
            }
        }
        "grep" | "grep.exe" | "egrep" | "egrep.exe" | "fgrep" | "fgrep.exe" => {
            let (query, path) = native_grep_args(args)?;
            ParsedCommand::Search {
                cmd: script.to_owned(),
                query: Some(query),
                path: path.as_deref().map(basename),
            }
        }
        "select-object" | "select" => {
            numeric_selector(args, quoted)?;
            return Some(None);
        }
        _ => return None,
    };
    Some(Some(action))
}

fn cmdlet_path(
    args: &[String],
    quoted: &[bool],
    switches: &[&str],
    numbers: &[&str],
    values: &[&str],
) -> Option<String> {
    let mut path = None;
    let mut literal = false;
    let mut seen = HashSet::new();
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if let Some(parameter) = arg.strip_prefix('-').filter(|_| !quoted[index]) {
            let parameter = parameter.to_ascii_lowercase();
            if !seen.insert(parameter.clone()) {
                return None;
            }
            if matches!(parameter.as_str(), "path" | "literalpath") {
                if path.is_some() {
                    return None;
                }
                literal = parameter == "literalpath";
                index += 1;
                path = Some(args.get(index)?.clone());
            } else if parameter == "erroraction" {
                index += 1;
                error_action(args.get(index)?)?;
            } else if switches.contains(&parameter.as_str()) {
                // A switch must not accidentally consume the following path.
            } else if numbers.contains(&parameter.as_str()) {
                index += 1;
                numeric(args.get(index)?)?;
            } else if values.contains(&parameter.as_str()) {
                index += 1;
                simple_value(args.get(index)?)?;
            } else {
                return None;
            }
        } else if path.replace(arg.clone()).is_some() {
            return None;
        }
        index += 1;
    }
    let path = path?;
    valid_path(&path, literal).then_some(path)
}

fn listing_args(args: &[String], quoted: &[bool]) -> Option<(Option<String>, Option<String>)> {
    let mut path = None;
    let mut filter = None;
    let mut literal = false;
    let mut seen = HashSet::new();
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if let Some(parameter) = arg.strip_prefix('-').filter(|_| !quoted[index]) {
            let parameter = parameter.to_ascii_lowercase();
            if !seen.insert(parameter.clone()) {
                return None;
            }
            match parameter.as_str() {
                "path" | "literalpath" => {
                    if path.is_some() {
                        return None;
                    }
                    literal = parameter == "literalpath";
                    index += 1;
                    path = Some(args.get(index)?.clone());
                }
                "filter" => {
                    if filter.is_some() {
                        return None;
                    }
                    index += 1;
                    filter = Some(args.get(index)?.clone());
                }
                "file" | "directory" | "recurse" | "force" | "name" => {}
                "erroraction" => {
                    index += 1;
                    error_action(args.get(index)?)?;
                }
                "depth" => {
                    index += 1;
                    numeric(args.get(index)?)?;
                }
                _ => return None,
            }
        } else if path.replace(arg.clone()).is_some() {
            return None;
        }
        index += 1;
    }
    if path
        .as_deref()
        .is_some_and(|value| !valid_path(value, literal))
    {
        return None;
    }
    if filter.as_deref().is_some_and(|value| {
        value.is_empty() || value.contains(['$', '`', ';', '|', '&', '>', '<'])
    }) {
        return None;
    }
    Some((path, filter))
}

fn select_string_args(args: &[String], quoted: &[bool]) -> Option<(String, Option<String>)> {
    let mut query = None;
    let mut path = None;
    let mut literal = false;
    let mut seen = HashSet::new();
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if let Some(parameter) = arg.strip_prefix('-').filter(|_| !quoted[index]) {
            let parameter = parameter.to_ascii_lowercase();
            if !seen.insert(parameter.clone()) {
                return None;
            }
            match parameter.as_str() {
                "pattern" => {
                    if query.is_some() {
                        return None;
                    }
                    index += 1;
                    query = Some(args.get(index)?.clone());
                }
                "path" | "literalpath" => {
                    if path.is_some() {
                        return None;
                    }
                    literal = parameter == "literalpath";
                    index += 1;
                    path = Some(args.get(index)?.clone());
                }
                "simplematch" | "casesensitive" | "allmatches" | "quiet" | "list" | "notmatch" => {}
                "erroraction" => {
                    index += 1;
                    error_action(args.get(index)?)?;
                }
                "context" => {
                    index += 1;
                    numeric(args.get(index)?)?;
                }
                "encoding" => {
                    index += 1;
                    simple_value(args.get(index)?)?;
                }
                _ => return None,
            }
        } else if query.is_none() {
            query = Some(arg.clone());
        } else if path.replace(arg.clone()).is_some() {
            return None;
        }
        index += 1;
    }
    let query = query.filter(|value| !value.is_empty())?;
    if path
        .as_deref()
        .is_some_and(|value| !valid_path(value, literal))
    {
        return None;
    }
    Some((query, path))
}

fn numeric(value: &str) -> Option<()> {
    (!value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit())).then_some(())
}

fn simple_value(value: &str) -> Option<()> {
    (!value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-'))
    .then_some(())
}

fn error_action(value: &str) -> Option<()> {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "stop" | "continue" | "silentlycontinue" | "ignore" | "inquire" | "break" | "suspend"
    )
    .then_some(())
}

fn valid_path(path: &str, literal: bool) -> bool {
    if path.is_empty()
        || path.starts_with(['~', '-'])
        || path.contains(['*', '?', '|', ';', '&', '>', '<', '$', '`'])
    {
        return false;
    }
    if !literal && path.contains(['[', ']']) {
        return false;
    }
    // Permit only a single colon after an ASCII drive letter. This excludes providers,
    // alternate data streams, and drive-relative paths such as C:secret.
    let mut colons = path.match_indices(':');
    if let Some((index, _)) = colons.next()
        && (index != 1
            || colons.next().is_some()
            || !path.as_bytes()[0].is_ascii_alphabetic()
            || !path
                .as_bytes()
                .get(2)
                .is_some_and(|byte| matches!(*byte, b'/' | b'\\')))
    {
        return false;
    }
    true
}

fn basename(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let trimmed = normalized.trim_end_matches('/');
    trimmed
        .split('/')
        .rev()
        .find(|part| {
            !part.is_empty() && !matches!(*part, "build" | "dist" | "node_modules" | "src")
        })
        .unwrap_or(trimmed)
        .to_owned()
}

fn numeric_selector(args: &[String], quoted: &[bool]) -> Option<()> {
    let mut index = 0;
    let mut found = false;
    while let Some(arg) = args.get(index) {
        let parameter = arg
            .strip_prefix('-')
            .filter(|_| !quoted[index])?
            .to_ascii_lowercase();
        if !matches!(parameter.as_str(), "skip" | "first" | "last") {
            return None;
        }
        index += 1;
        numeric(args.get(index)?)?;
        found = true;
        index += 1;
    }
    found.then_some(())
}

fn rg_args(args: &[String]) -> Option<(bool, Option<String>, Option<String>)> {
    let mut files = false;
    let mut pattern = None;
    let mut operands = Vec::new();
    let mut index = 0;
    let mut after_dash = false;
    while let Some(arg) = args.get(index) {
        if after_dash {
            operands.push(arg.clone());
        } else if arg == "--" {
            after_dash = true;
        } else if matches!(
            arg.as_str(),
            "--files"
                | "-l"
                | "--files-with-matches"
                | "-n"
                | "--line-number"
                | "-i"
                | "--ignore-case"
                | "-S"
                | "--smart-case"
                | "-F"
                | "--fixed-strings"
                | "-w"
                | "--word-regexp"
                | "-v"
                | "--invert-match"
                | "--hidden"
                | "--no-ignore"
                | "--pcre2"
                | "--multiline"
                | "--count"
                | "--json"
        ) {
            files |= arg == "--files";
        } else if matches!(arg.as_str(), "-e" | "--regexp") {
            index += 1;
            let value = args.get(index)?;
            if pattern.is_some() {
                return None;
            }
            pattern = Some(value.clone());
        } else if matches!(
            arg.as_str(),
            "-g" | "--glob"
                | "--iglob"
                | "-t"
                | "--type"
                | "-m"
                | "--max-count"
                | "-A"
                | "-B"
                | "-C"
                | "--context"
                | "--max-depth"
        ) {
            index += 1;
            args.get(index)?;
        } else if arg.starts_with("--glob=")
            || arg.starts_with("--iglob=")
            || arg.starts_with("--type=")
            || arg.starts_with("--max-depth=")
        {
            if arg.ends_with('=') {
                return None;
            }
        } else if arg.starts_with('-') {
            return None;
        } else {
            operands.push(arg.clone());
        }
        index += 1;
    }
    if files {
        if pattern.is_some() || operands.len() > 1 {
            return None;
        }
        let path = operands.pop();
        if path
            .as_deref()
            .is_some_and(|value| !valid_path(value, false))
        {
            return None;
        }
        Some((true, None, path))
    } else {
        let query = pattern.or_else(|| {
            if operands.is_empty() {
                None
            } else {
                Some(operands.remove(0))
            }
        });
        let query = query.filter(|value| !value.is_empty())?;
        if operands.len() > 1 {
            return None;
        }
        let path = operands.pop();
        if path
            .as_deref()
            .is_some_and(|value| !valid_path(value, false))
        {
            return None;
        }
        Some((false, Some(query), path))
    }
}

fn native_reader_path(command: &str, args: &[String]) -> Option<String> {
    let mut path = None;
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        let consumes_value = if command.starts_with("bat") {
            matches!(
                arg.as_str(),
                "--theme" | "--language" | "--style" | "--line-range" | "--terminal-width"
            )
        } else if command.starts_with("less") {
            matches!(arg.as_str(), "-p" | "-P" | "--pattern" | "--prompt")
        } else if command.starts_with("head") || command.starts_with("tail") {
            matches!(arg.as_str(), "-n" | "--lines")
        } else {
            false
        };
        if consumes_value {
            index += 1;
            let value = args.get(index)?;
            if command.starts_with("head") || command.starts_with("tail") {
                numeric(value.strip_prefix('+').unwrap_or(value))?;
            } else if value.is_empty() {
                return None;
            }
        } else if ((command.starts_with("head") || command.starts_with("tail"))
            && arg.strip_prefix("-n").is_some_and(|value| {
                !value.is_empty() && numeric(value.strip_prefix('+').unwrap_or(value)).is_some()
            }))
            || (command.starts_with("bat")
                && matches!(arg.as_str(), "-n" | "-p" | "--plain" | "--paging=never"))
            || (command.starts_with("less")
                && matches!(arg.as_str(), "-N" | "-R" | "-F" | "-X" | "-S"))
        {
        } else if arg.starts_with('-') || path.replace(arg.clone()).is_some() {
            return None;
        }
        index += 1;
    }
    let path = path?;
    valid_path(&path, false).then_some(path)
}

fn native_grep_args(args: &[String]) -> Option<(String, Option<String>)> {
    let mut query = None;
    let mut path = None;
    let mut index = 0;
    while let Some(arg) = args.get(index) {
        if matches!(
            arg.as_str(),
            "-i" | "-n"
                | "-r"
                | "-R"
                | "-F"
                | "-w"
                | "-v"
                | "-l"
                | "-H"
                | "--ignore-case"
                | "--line-number"
                | "--recursive"
                | "--fixed-strings"
                | "--word-regexp"
        ) {
        } else if matches!(arg.as_str(), "-e" | "--regexp") {
            if query.is_some() {
                return None;
            }
            index += 1;
            query = Some(args.get(index)?.clone());
        } else if matches!(
            arg.as_str(),
            "-A" | "-B" | "-C" | "--after-context" | "--before-context" | "--context"
        ) {
            index += 1;
            numeric(args.get(index)?)?;
        } else if arg.starts_with('-') {
            return None;
        } else if query.is_none() {
            query = Some(arg.clone());
        } else if path.replace(arg.clone()).is_some() {
            return None;
        }
        index += 1;
    }
    let query = query.filter(|value| !value.is_empty())?;
    if path
        .as_deref()
        .is_some_and(|value| !valid_path(value, false))
    {
        return None;
    }
    Some((query, path))
}

fn git_action(args: &[String], script: &str) -> Option<ParsedCommand> {
    let (subcommand, tail) = args.split_first()?;
    if subcommand == "ls-files" {
        let path = match tail {
            [] => None,
            [path] if valid_path(path, false) => Some(basename(path)),
            _ => return None,
        };
        return Some(ParsedCommand::ListFiles {
            cmd: script.to_owned(),
            path,
        });
    }
    if subcommand != "grep" {
        return None;
    }
    let mut query = None;
    let mut path = None;
    let mut index = 0;
    while let Some(arg) = tail.get(index) {
        if matches!(
            arg.as_str(),
            "-n" | "-i"
                | "-I"
                | "-w"
                | "-F"
                | "--fixed-strings"
                | "--line-number"
                | "--ignore-case"
        ) {
        } else if matches!(arg.as_str(), "-e" | "--regexp") {
            index += 1;
            if query.is_some() {
                return None;
            }
            query = Some(tail.get(index)?.clone());
        } else if arg == "--" {
            index += 1;
            if path.is_some() || index + 1 != tail.len() {
                return None;
            }
            path = Some(tail.get(index)?.clone());
        } else if arg.starts_with('-') {
            return None;
        } else if query.is_none() {
            query = Some(arg.clone());
        } else if path.replace(arg.clone()).is_some() {
            return None;
        }
        index += 1;
    }
    let query = query.filter(|value| !value.is_empty())?;
    if path
        .as_deref()
        .is_some_and(|value| !valid_path(value, false))
    {
        return None;
    }
    Some(ParsedCommand::Search {
        cmd: script.to_owned(),
        query: Some(query),
        path: path.as_deref().map(basename),
    })
}

#[cfg(test)]
#[path = "powershell_presentation_tests.rs"]
mod tests;
