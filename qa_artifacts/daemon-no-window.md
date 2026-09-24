# Windows daemon console-window regression evidence

Worktree: E:\orca\workspaces\codex-powershell-ui\codex-daemon-no-window
Base: fe8acacab0c72665e826b96ca4d6d48c3163574e (rust-v0.158.0-alpha.5)

## Before the fix

A temporary Win32 parent/child Python harness ran twice with CREATE_BREAKAWAY_FROM_JOB successfully applied in both cases.

| Parent creation flag | Parent console HWND | Unflagged child console HWND | Visible |
| --- | ---: | ---: | --- |
| DETACHED_PROCESS | null | 38407370 (repeat: 4327782) | true on both runs |
| CREATE_NO_WINDOW | null | null on both runs | false |

The original launch flags therefore left the daemon console-less but allowed an ordinary console child to create a visible console window. CREATE_NO_WINDOW propagated the no-window behavior to the child.

The new Rust regression was added before changing the production flag. It failed twice under the local nextest retry profile with child HWNDs 0x612fe and 0x13f1088.

## Change and after checks

PidBackend and ensure_detached_launch now share CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB; the breakaway bit stays intact. The focused test launches a process with the shared console flag and verifies that its default child receives a null console HWND.

- just test -p codex-app-server-daemon --lib daemon_default_children_do_not_open_console_windows after the fix: 1 passed.
- just test -p codex-app-server-daemon --lib: 39 passed, 0 skipped. This includes the existing restricted-job preflight and daemon lifecycle cases.
- Targeted formatting: cargo fmt -p codex-app-server-daemon -- --config imports_granularity=Item, then the same command with --check: both exit 0. Stable rustfmt warns that imports_granularity is nightly-only.
- Repository-wide just fmt was attempted twice and remains unavailable in this Windows worktree: with UTF-8 enabled, Bazel/Starlark formatting could not start because the dotslash buildifier launcher returned WinError 2, and workspace cargo fmt failed with Windows OS error 206 (path too long). The affected Rust crate was formatted and checked directly.
- The first filtered build took about eight minutes while the pinned Rust 1.95 toolchain and dependencies were fetched; the warmed repeat compiled the crate in 28.41 seconds. The subsequent full Windows CLI build passed in 21m38s. An isolated daemon start, socket/version probe, zero console-window handle, and graceful stop passed for commit 431c7f190c. That staged binary contains the console fix only; it is not evidence of per-client hook ownership.

## Shared-daemon hook ownership (T003)

The local TUI sends a bounded, transient hook owner on start, fork, and resume.
Internal children share their root owner; public forks get a separate owner.
Resume checks subscriptions for the whole loaded family, preventing a second
pane from taking ownership through an unsubscribed child. Only authenticated
local daemon transport accepts this metadata. Missing optional values clear
inherited daemon values; hook auth tokens and arbitrary environment are excluded.
Older daemons must advertise support before an Orca client uses this path.

Verified before installation:
- Protocol library: 344 tests passed.
- Hooks library: 181 tests passed (t003-hooks-tests-final.log). A test-only
  python3 shim supplies the installed Python on Windows; no global PATH change.
- App-server protocol: 312 passed, one intentionally ignored schema writer
  (t003-app-server-protocol-tests-final.log). Stable and experimental schemas
  were regenerated; checked-in fixtures match the generator.
- Independent review found a family-ownership conflict. The corrected family
  subscription check and three subsequent plumbing additions were rechecked
  against frozen hashes, with no remaining actionable P1/P2 findings.

Core/app-server/client/transport/TUI: 12 focused tests passed across five binaries
(t003-owner-final-check.log); 8431 unrelated tests were filtered out. This covers
the loaded child family, public-fork isolation, active subscriber conflict,
all four TUI request shapes, remote exclusion, old-server capability handling,
and local control socket transport.

Scoped formatting and just fix for the eight changed packages completed
successfully. Plain Clippy for hooks also passed without fix-mode lint capping
(t003-hooks-clippy-final.log). The intentional read guard across hook execution
has function-local lint expectations documenting the bounded handoff barrier;
the corresponding timeout test documents its intentional held guard as well.
No runtime behavior or lock lifetime was weakened. Two unused test-helper
warnings and the expanded initializer argument-count warning remain.

Final CLI build passed in 13m 54s from source commit c5d9b253ab7481ec3e9add8a272bd9ac24873304. Staged codex.exe SHA256: 9CDA3D4428D2D8EF6D8DE9D388EDB7B8136E40C06C2377BD14C48A9A353DC0D9. The package was installed through daemon update --from-cli --yes; the installed SHA matches. Bootstrap passed and the managed daemon is running.
Live code-mode returned T003_CODEMODE_LIVE_PASS (session 01a0d1fc-239e-75e2-aba2-071046d52a2e). The code-mode host was a direct child of the installed daemon. A 120-second window watch spanning bootstrap, MCP startup and code-mode found no new visible console windows; this does not detect new tabs inside an existing Terminal window. A second live TUI used the exact installed managed CLI (verified executable path), gpt-6-astra/low, and returned T003_MANAGED_CLIENT_PASS (session 01a0d200-3be8-7e10-b872-318e9be0f7cb). The isolated A/B/public-fork test passed all six checks; three SessionStart recorders stopped their turns before inference. The config and rollout sentinel scan ran after stopping the isolated daemon. Internal-child ownership remains covered by the focused core test, not this runtime probe. Orca was restarted in background with the managed CLI command override and --no-daemon removed; settings were read back from the restarted runtime. Compact runtime evidence is in daemon-hook-owner-runtime.json.
