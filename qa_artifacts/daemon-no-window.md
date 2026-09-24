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
- The first filtered build took about eight minutes while the pinned Rust 1.95 toolchain and dependencies were fetched; the warmed repeat compiled the crate in 28.41 seconds. No full Codex build, active CLI/daemon change, or restart was performed.
