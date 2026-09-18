use codex_protocol::parse_command::ParsedCommand;
use codex_shell_command::parse_command::parse_command;
use pretty_assertions::assert_eq;

fn actions(shell: &str, script: &str) -> Vec<ParsedCommand> {
    let flag = if shell == "pwsh" { "-Command" } else { "-lc" };
    parse_command(&[shell.to_owned(), flag.to_owned(), script.to_owned()])
        .into_iter()
        .map(|action| match action {
            ParsedCommand::Read { name, path, .. } => ParsedCommand::Read {
                cmd: String::new(),
                name,
                path,
            },
            ParsedCommand::Search { query, path, .. } => ParsedCommand::Search {
                cmd: String::new(),
                query,
                path,
            },
            ParsedCommand::ListFiles { path, .. } => ParsedCommand::ListFiles {
                cmd: String::new(),
                path,
            },
            ParsedCommand::Unknown { .. } => ParsedCommand::Unknown { cmd: String::new() },
        })
        .collect()
}

#[test]
fn shell_wrappers_preserve_equivalent_action_data() {
    let pairs = [
        (
            "cat README.md",
            "Get-Content -LiteralPath README.md -Encoding utf8",
        ),
        (
            "cat README.md",
            "GET-CONTENT -LITERALPATH README.md -READCOUNT 20",
        ),
        ("cat README.md", "type README.md"),
        ("cat README.md", "cat README.md"),
        ("head -n 20 README.md", "gc README.md -Head 20"),
        ("tail -n 20 README.md", "gc README.md -Last 20"),
        (
            "head -n 160 README.md",
            "Get-Content -LiteralPath README.md -TotalCount 160",
        ),
        ("tail -n 20 README.md", "Get-Content README.md -Tail 20"),
        (
            "sed -n '11,30p' README.md",
            "Get-Content README.md | Select-Object -Skip 10 -First 20",
        ),
        ("rg -n TODO fixture", "rg -n TODO fixture"),
        (r"rg '\bTODO\b' fixture", r"rg '\bTODO\b' fixture"),
        (
            "grep -n TODO README.md",
            "Select-String -LiteralPath README.md -Pattern TODO",
        ),
        ("rg --files fixture", "rg --files fixture"),
        ("ls fixture", "Get-ChildItem -LiteralPath fixture"),
        ("ls fixture", "gci fixture -File -Force -Recurse"),
        ("eza fixture", "dir fixture"),
        ("exa fixture", "ls fixture"),
        (
            "find fixture -name '*.rs'",
            "Get-ChildItem fixture -Recurse -Filter '*.rs'",
        ),
        ("ag TODO fixture", "sls -Pattern TODO -Path fixture"),
        ("ack TODO fixture", "rg TODO fixture"),
        ("pt TODO fixture", "rg TODO fixture"),
        ("rga TODO fixture", "rga TODO fixture"),
        ("ripgrep-all TODO fixture", "ripgrep-all TODO fixture"),
        ("git ls-files fixture", "git ls-files fixture"),
        ("git grep TODO fixture", "git grep TODO fixture"),
        (
            "cat a.md; rg TODO fixture; ls fixture",
            "gc a.md; rg TODO fixture; dir fixture",
        ),
        (
            "cat skills/demo/SKILL.md",
            "Get-Content skills/demo/SKILL.md -TotalCount 160",
        ),
        ("grep -e '-TODO' fixture", "rg -e '-TODO' fixture"),
        (
            "grep -e '-TODO' README.md",
            "Select-String -Pattern '-TODO' -LiteralPath README.md",
        ),
        (
            "rg 'TODO|FIXME;done' fixture | head -n 5",
            "rg 'TODO|FIXME;done' fixture | Select-Object -First 5",
        ),
    ];
    let mut mismatches = Vec::new();
    for (bash, powershell) in pairs {
        let expected = actions("bash", bash);
        assert_eq!(actions("zsh", bash), expected, "zsh: {bash}");
        let actual = actions("pwsh", powershell);
        if actual != expected {
            mismatches.push(format!(
                "{bash:?} vs {powershell:?}\nexpected {expected:?}\nactual {actual:?}"
            ));
        }
    }
    assert!(mismatches.is_empty(), "{}", mismatches.join("\n\n"));
}

#[test]
fn powershell_wrapper_trailing_arguments_cannot_hide_mutations() {
    let command = [
        "pwsh",
        "-Command",
        "Get-Content README.md",
        "; Set-Content other.md changed",
    ]
    .map(str::to_owned);
    assert_eq!(
        parse_command(&command),
        vec![ParsedCommand::Unknown {
            cmd: codex_shell_command::parse_command::shlex_join(&command),
        }]
    );
}
