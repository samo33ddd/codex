# Codex PowerShell Actions

**Readable file actions for Codex on Windows.**

[![Windows checks](https://github.com/samo33ddd/codex/actions/workflows/powershell-parity.yml/badge.svg?branch=powershell-action-parity)](https://github.com/samo33ddd/codex/actions/workflows/powershell-parity.yml)
[![Upstream compatibility](https://github.com/samo33ddd/codex/actions/workflows/powershell-upstream.yml/badge.svg?branch=powershell-action-parity)](https://github.com/samo33ddd/codex/actions/workflows/powershell-upstream.yml)

[Get a candidate build](#use-a-candidate-build) · [Compatibility](#compatibility) · [Supported commands](#what-changes) · [Verification](verification/powershell-parity.md)

An unofficial, version-pinned backend patch for **Windows x64 and PowerShell 7**. Recognized file reads, searches, and directory listings appear as structured actions in the Codex activity history, with file links where supported.

| PowerShell command | Activity |
|---|---|
| `Get-Content README.md` | Read a file and open its preview |
| `Select-String -Path README.md -Pattern TODO` | Search file contents |
| `Get-ChildItem` | List files |

## In action

![English Codex interface showing PowerShell read, search and list actions beside the opened README](verification/desktop-readme.png)

*A real desktop capture: expanded PowerShell activity on the left, the file opened from a read action on the right. Captured with the English interface in an isolated test profile.*

This fork preserves the history of [openai/codex](https://github.com/openai/codex); the patch lives on `powershell-action-parity`. It is not an OpenAI release or a desktop plugin. The installed desktop resources are unchanged.

## Compatibility

| Component | Verified baseline |
|---|---|
| Windows desktop app | `26.911.7940.0` |
| Backend version | `0.155.0-alpha.2.6` |
| Upstream source tag | `rust-v0.155.0-alpha.2.6` |
| Upstream source commit | `bf6f0a4ec97919bf697cdc532e7b8af4ec482fc6` |
| Platform | Windows x64, PowerShell 7 |
| Rust toolchain | `1.95.0` |

The launcher follows the installed desktop app. On first use or after an app update, it automatically prepares a new local bundle and verifies its executable, UI archive and custom backend. Desktop version numbers are recorded baselines, not a launch allowlist. The custom backend remains pinned to the version in [compatibility.json](scripts/powershell-parity/compatibility.json); this does not download or build new backend releases.

## What changes

- `Get-Content`, `gc`, `type`, `cat`: literal file reads, explicit path arguments, `Encoding`, `Raw`, `ReadCount`, first/last line limits.
- A narrow `Get-Content ... | Select-Object -Skip ... -First ...` pipeline preserves the read action.
- `Select-String` / `sls`, supported `rg` / `rga` / `grep` arguments, and `git grep`: content searches with preserved literal regexes.
- `Get-ChildItem` / `gci` / `dir` / `ls`, `rg --files`, and `git ls-files`: file listing; supported filename filters follow the existing search-action convention.
- Ordered literal command chains keep every recognized action. Skill-file reads use the same conservative parser.

Variables, interpolation, computed calls, redirection, directory changes, unsupported pipeline stages, and mixed read/mutation scripts remain generic commands. Classification does not execute PowerShell, expand another process's environment, change command output, or grant safety/approval privileges.

See [coverage and verification](verification/powershell-parity.md) for precise boundaries and known limits.

## Use a candidate build

[PowerShell parity Actions](https://github.com/samo33ddd/codex/actions/workflows/powershell-parity.yml) produces a candidate ZIP after the Windows checks pass. Candidates are not automatically published as releases. The launcher automatically refreshes its local desktop copy when the installed app changes.

1. Download and extract a candidate artifact. It includes SHA-256 checksums, a build manifest, the launcher, three locally built executables, and the pinned official `codex-code-mode-host.exe`.
2. Install the Codex desktop app, Python 3.11+ and Node.js. Keep the preparation scripts included in the candidate directory.
3. In PowerShell 7, prepare and validate the desktop and components from the extracted candidate directory:

   ```powershell
   .\Start-Codex.ps1 -ValidateOnly
   ```

4. Save your work and close Codex yourself. Then run:

   ```powershell
   .\Start-Codex.ps1
   ```

The launcher sets `CODEX_CLI_PATH` only for the new process and checks the actual backend process path. It does not close an existing session or edit account credentials, providers, models, global environment variables, WindowsApps, or app.asar.

The native Windows bootstrap can lose the backend environment override on the normal profile. The launcher therefore uses a prepared desktop bundle with the verified custom backend in its own resources. It reads `desktop-bundle.json` beside the backend package and compares the registered source archive and executable hashes with the installed app. If the bundle is missing or outdated, it prepares a new directory under `desktop-bundles` beside the backend. Successful preparation updates the registration atomically; failed preparation preserves it and stops the launch. Existing bundles are retained. An explicit `-DesktopPath` selects a fixed bundle and must match the installed app. The launcher passes `--user-data-dir` explicitly and validates the actual backend even if the initial process exits.

**Rollback:** close this instance and start Codex from its usual Start-menu shortcut. The stock installation is unchanged.

## Local desktop bundle and Git labels

The preparation script creates a separate local copy of the installed desktop and includes all four verified backend executables. Git activity labels cover literal `status`, `diff`, `log`, `show`, `branch`, `remote`, `rev-parse`, `ls-remote` and `ls-files` commands with supported flags and selected `-c`/`-C` options. Semicolon and newline chains of Git commands receive a Git summary; supported mixtures with file reads or `rg` explicitly mention other commands. The original command and output remain available by expanding the row. Unsupported syntax, errors and interruptions keep the standard presentation. These labels do not change backend classifications or permissions.

Ordinary launches update automatically. To prepare a bundle in a chosen directory, with Node.js and Python available:

```powershell
$app = (Get-AppxPackage -Name OpenAI.Codex).InstallLocation + '\app'
python scripts/powershell-parity/prepare_desktop.py --source-app $app --backend-dir artifacts/powershell-parity --output artifacts/git-desktop/app

# After closing Codex:
pwsh -NoProfile -File scripts/powershell-parity/Start-Codex.ps1
```

Preparation requires a new output directory and verifies each backend component against the backend build manifest. Git label overlays use exact SHA-256 checks for the supported UI assets in desktop versions `26.911.7940.0` and `26.915.4065.0`. If a future desktop has unknown assets, preparation retains its standard Git labels and the launcher prints a warning. PowerShell read/search/list classifications still come from the custom backend. Preparation records component hashes, changed asset hashes, both ASAR hashes and the Git label mode in `desktop-patch-manifest.json`. After successful preparation it atomically writes `desktop-bundle.json` in the backend directory. The launcher verifies the desktop manifest before starting and checks the running backend path. The installed desktop is not modified. Future desktop/backend protocol changes can still require a backend update; automatic copying is not a guarantee of compatibility with every future release.

`-UserDataPath` provides a separate native browser and Electron profile for testing. Ordinary launches retain the normal profile. `-ValidateOnly` prepares an update if needed and checks it without starting the app. Local verification uses `test_launcher.ps1`, `test_prepare_desktop.py` and `test_desktop_git_labels.cjs`; the latter requires the matching assets extracted under `.local/desktop`. Add `--current` to also exercise the `26.915.4065.0` assets extracted under `.local/desktop-current`.

## Build and verify locally

Prerequisites: Windows x64, PowerShell 7, Python 3.11+, Git, Visual Studio C++ build tools and Windows SDK, Rust/rustup, `just`, `cargo-nextest`, and `rg`. Install missing system components deliberately. The optional `Enter-BuildEnvironment.ps1` keeps Rust homes and build output under a supplied directory; it changes only the current shell environment.

From the repository root:

```powershell
# Optional isolated toolchain location; set a directory on the desired drive.
. .\scripts\powershell-parity\Enter-BuildEnvironment.ps1 -BuildRoot 'E:\codex-build-tools'
rustup toolchain install 1.95.0 --component clippy --component rustfmt --component rust-src
cargo install --locked just --version 1.58.0
cargo install --locked cargo-nextest --version 0.9.145

just test -p codex-shell-command -p codex-skills -p codex-app-server-protocol
just test -p codex-tui --lib -E 'test(exec_cell::render::tests::powershell_skill_read_snapshot)'
Push-Location codex-rs
cargo build --release -p codex-cli -p codex-windows-sandbox --bins
Pop-Location

python scripts/powershell-parity/check_app_server.py --binary "$env:CARGO_TARGET_DIR/release/codex.exe" --output .local/appserver-check
python scripts/powershell-parity/package_windows.py --binary-dir "$env:CARGO_TARGET_DIR/release" --output artifacts/codex-powershell-actions-windows-x64 --profile release
```

The public app-server check uses a loopback model fixture and a disposable `CODEX_HOME`; it executes only the explicit read-only fixture commands. It needs no API credentials. Packaging downloads the exact official code-mode host and rejects a SHA-256 mismatch. This avoids the unavailable Windows V8 archive in this source tag without disabling its sandbox.

This upstream tag normalizes local workspace package versions in Cargo.lock during builds. Do not include that unrelated normalization in a patch update. The baseline's full workspace tests also require dependencies beyond this scoped workflow.

## Branches and updates

- `main`: clean upstream snapshot; do not merge the patch here.
- `powershell-action-parity`: default branch for the patch, documentation, CI, and update checks.
- `ps-actions/0.155.0-alpha.2.6`: maintenance branch for the recorded source baseline.

The [upstream compatibility workflow](https://github.com/samo33ddd/codex/actions/workflows/powershell-upstream.yml) runs every Monday at 06:37 UTC and can be started manually with a source tag. It checks patch applicability using an isolated Git index. It never checks out or executes candidate source, rewrites a branch, pushes, or updates a desktop installation. Its summary and JSON artifact distinguish an applicable patch, an already-applied patch, and a conflict. A conflict is a report outcome, not a claim that tests passed.

For a backend upgrade, or a desktop update that changes its backend protocol:

1. Identify its actual bundled backend version and confirm the override mechanism still exists.
2. Create a new version branch from the matching official source tag; cherry-pick the backend patch and adapt conflicts.
3. Carry over the delivery tooling, update compatibility metadata and the official host hash, and run Windows CI.
4. Test an isolated desktop instance: labels, ordering/grouping, actual file links, skills and relevant MCP/tools.
5. Publish a release only after recording those results. Attach the tested package, checksums, and compatibility notes; retain previous tags for rollback.

The two PowerShell workflows use standard hosted runners and do not require OpenAI's signing secrets or private runner groups. Existing fork branches and inherited workflow settings are preserved; pushing the patch branches does not trigger the upstream main-branch or tag-release workflows.

## License

[Apache-2.0](LICENSE), with the upstream [NOTICE](NOTICE) retained. Upstream documentation is available in [openai/codex](https://github.com/openai/codex/tree/rust-v0.155.0-alpha.2.6).
