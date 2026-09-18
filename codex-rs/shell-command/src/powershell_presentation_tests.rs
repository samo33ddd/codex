use super::parse_powershell_script;
use crate::parse_command::parse_command;
use codex_protocol::parse_command::ParsedCommand;
use pretty_assertions::assert_eq;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
enum ActionShape {
    Read {
        name: String,
        path: PathBuf,
    },
    Search {
        query: Option<String>,
        path: Option<String>,
    },
    ListFiles {
        path: Option<String>,
    },
    Unknown,
}

fn action_shapes(commands: Vec<ParsedCommand>) -> Vec<ActionShape> {
    commands
        .into_iter()
        .map(|command| match command {
            ParsedCommand::Read { name, path, .. } => ActionShape::Read { name, path },
            ParsedCommand::Search { query, path, .. } => ActionShape::Search { query, path },
            ParsedCommand::ListFiles { path, .. } => ActionShape::ListFiles { path },
            ParsedCommand::Unknown { .. } => ActionShape::Unknown,
        })
        .collect()
}

fn shape(commands: Vec<ParsedCommand>) -> Vec<String> {
    commands
        .into_iter()
        .map(|command| match command {
            ParsedCommand::Read { path, .. } => format!("read:{}", path.display()),
            ParsedCommand::Search { query, path, .. } => {
                format!("search:{query:?}:{path:?}")
            }
            ParsedCommand::ListFiles { path, .. } => format!("list:{path:?}"),
            ParsedCommand::Unknown { .. } => "unknown".to_owned(),
        })
        .collect()
}

#[test]
fn literal_powershell_actions_match_bash_and_zsh_fields() {
    for (bash, powershell) in [
        ("cat a.txt", "Get-Content a.txt"),
        ("ls src", "Get-ChildItem src"),
        ("rg TODO src", "Select-String -Pattern TODO -Path src"),
        ("rg --files src", "rg --files src"),
        (
            "find src -name '*.rs'",
            "Get-ChildItem -Path src -Filter '*.rs'",
        ),
        ("git grep TODO src", "git grep TODO src"),
        ("git ls-files src", "git ls-files src"),
        ("bat a.txt", "bat a.txt"),
        ("grep TODO src", "grep TODO src"),
        ("head -n 20 a.txt", "head -n 20 a.txt"),
        (
            "cat a.txt | head -n 20",
            "Get-Content a.txt | Select-Object -First 20",
        ),
        ("cat a.txt; rg TODO src", "Get-Content a.txt; rg TODO src"),
    ] {
        let expected = action_shapes(parse_command(&["bash".into(), "-lc".into(), bash.into()]));
        assert_eq!(
            action_shapes(parse_powershell_script(powershell)),
            expected,
            "{powershell}"
        );
        let zsh = action_shapes(parse_command(&["zsh".into(), "-lc".into(), bash.into()]));
        assert_eq!(zsh, expected, "{bash}");
    }
}

#[test]
fn powershell_read_search_and_list_match_bash_categories() {
    assert_eq!(
        shape(parse_powershell_script(
            r"Get-Content -LiteralPath C:\work\a.txt -Raw"
        )),
        vec!["read:C:/work/a.txt"]
    );
    assert_eq!(
        shape(parse_powershell_script(
            "Get-ChildItem -Path src -Filter '*.rs'"
        )),
        vec!["search:Some(\"*.rs\"):Some(\"src\")"]
    );
    assert_eq!(
        shape(parse_powershell_script(
            "Select-String -Pattern TODO -Path src/a.rs"
        )),
        vec!["search:Some(\"TODO\"):Some(\"a.rs\")"]
    );
    assert_eq!(
        shape(parse_powershell_script("rg --files src")),
        vec!["list:Some(\"src\")"]
    );
    assert_eq!(
        shape(parse_powershell_script(r"rg '\bTODO\b' src")),
        vec![r#"search:Some("\\bTODO\\b"):Some("src")"#]
    );
}

#[test]
fn powershell_literal_chains_and_select_pipeline() {
    assert_eq!(
        shape(parse_powershell_script(
            "Get-Content a.txt | Select-Object -Skip 10 -First 20"
        )),
        vec!["read:a.txt"],
    );
    assert_eq!(
        shape(parse_powershell_script("Get-Content a.txt; rg TODO src")),
        vec!["read:a.txt", "search:Some(\"TODO\"):Some(\"src\")"],
    );
}

#[test]
fn powershell_unknown_stage_or_dynamic_syntax_keeps_whole_script_opaque() {
    for script in [
        "Get-Content a.txt | Set-Content b.txt",
        "Get-Content a.txt | Tee-Object b.txt",
        "Get-Content a.txt > b.txt",
        "Get-Content $env:TEMP",
        "Get-Content C:/work/file.txt:stream",
        "Get-Content -Path C:/work/[abc].txt",
        "rg --pre cat TODO src",
        "find . -exec rm {} ;",
        "Get-Content a.txt | Select-Object -Property { Remove-Item b.txt }",
        "Get-Content '-Raw' a.txt",
        "Get-Content `-Raw a.txt",
        "Get-Content a.txt; Select-Object -First 5",
        "Get-Content a.txt | Select-Object -First 5; Remove-Item b.txt",
        "rg -e TODO -e FIXME src",
        "Get-Content -ErrorVariable danger a.txt",
        "Select-String -OutVariable findings -Pattern TODO -Path src",
        "Get-Content -TotalCount 160",
        "Get-Content -TotalCount 10 -TotalCount 20 a.txt",
        "Get-Content -Path a.txt -LiteralPath b.txt",
        "grep -f patterns.txt src",
    ] {
        assert_eq!(
            shape(parse_powershell_script(script)),
            vec!["unknown"],
            "{script}"
        );
    }
}

#[test]
fn explicit_read_only_parameters_and_native_options_keep_fields() {
    assert_eq!(
        shape(parse_powershell_script(
            "Get-Content -Path a.txt -TotalCount 20 -Encoding UTF8 -ErrorAction Stop"
        )),
        vec!["read:a.txt"],
    );
    assert_eq!(
        shape(parse_powershell_script(
            "Get-ChildItem -Path src -Recurse -Depth 2 -ErrorAction Stop"
        )),
        vec!["list:Some(\"src\")"],
    );
    assert_eq!(
        shape(parse_powershell_script(
            "Select-String -Path src/a.rs -Pattern TODO -SimpleMatch -CaseSensitive -ErrorAction Stop"
        )),
        vec!["search:Some(\"TODO\"):Some(\"a.rs\")"],
    );
    assert_eq!(
        shape(parse_powershell_script(
            "rg.exe --glob '*.rs' -A 2 -B 1 TODO src"
        )),
        vec!["search:Some(\"TODO\"):Some(\"src\")"],
    );
}

#[test]
fn powershell_literal_brackets_and_numeric_pipeline_are_allowed() {
    assert_eq!(
        shape(parse_powershell_script(
            "Get-Content -LiteralPath 'C:/work/[abc].txt'"
        )),
        vec!["read:C:/work/[abc].txt"],
    );
    assert_eq!(
        shape(parse_powershell_script("gc a.txt | select -Last 5")),
        vec!["read:a.txt"],
    );
}
