# Custom alpha updater

Run from a scheduled task using a non-secret JSON config outside the repository:

```powershell
py -3 E:\path\to\scripts\custom-alpha\update.py E:\codex-build-tools\custom-alpha.json
```

Required config fields:

```json
{
  "sourceRepository": "E:/projects/codex-powershell-ui",
  "sourceRef": "origin/codex/custom-alpha",
  "scratchWorktreeRoot": "E:/codex-build-tools/custom-alpha-worktree",
  "artifactsRoot": "E:/codex-build-tools/custom-alpha-artifacts",
  "cargoTargetDir": "E:/codex-build-tools/target-codex-0155",
  "codexHomes": ["C:/Users/name/.codex"],
  "officialPackageTemplatePath": "E:/codex-build-tools/custom-alpha-templates",
  "upstreamRemote": "upstream",
  "upstreamTagPrefix": "rust-v"
}
```

The updater fetches the `@openai/codex` alpha tag and matching `@openai/codex-win32-x64` package, verifies npm SHA-512 integrity, and keeps versioned templates and candidates. It reuses only its marked isolated worktree, merges the custom source ref and `rust-v<version>`, and only resolves a sole `codex-rs/Cargo.toml` workspace version conflict. It runs `just test -p codex-cli` and `cargo build -p codex-cli --bin codex` with one Cargo job. Other conflicts, test failures, and build failures leave installed homes untouched.

Activation is deferred if a Codex client is running, the local-control RPC cannot prove every thread is not loaded, or any status is unknown. The read-only PowerShell 7 helper connects to `<CODEX_HOME>/app-server-control/app-server-control.sock`; if no managed daemon process exists after the client scan, there are no loaded in-memory threads and activation may proceed. A prepared candidate is tried first on the next run. After an updater install, matching the recorded last-known-good version and SHA-256 in every configured `CODEX_HOME` skips another build. Safe activation runs the candidate's `bin/codex.exe app-server daemon update --from-cli --yes` for each configured home; status and a bounded log are written under `artifactsRoot`.

Offline focused check: `py -3 scripts/custom-alpha/test_updater.py`.
