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

Final CLI build, package runtime validation and installation remain pending.
No installed T003 or live acceptance is claimed.
