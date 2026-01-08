# Nalias

Nalias is a lightweight, persistent command-alias manager for Windows. It compiles to one native Rust executable, stores definitions in one versioned JSON file, and exposes aliases to both Command Prompt and PowerShell through tiny generated `.cmd` wrappers.

This repository also contains [Nalias Lite](crates/nalias-lite/README.md), an independent dependency-light executable that embeds commands directly in wrappers. It is intended for users who prefer minimum binary size over centralized JSON and runtime dispatch.

| Capability | Nalias | Nalias Lite |
| --- | --- | --- |
| Release size in the current build | 551 KiB | 182 KiB |
| Wrapper behavior | Delegates to `nalias.exe run` | Executes the command directly |
| Source of truth | Versioned `aliases.json` | Generated `.cmd` wrapper |
| Shell modes | CMD, PowerShell, direct | CMD only |
| Runtime protection and diagnostics | Full | None |
| Management | Add, list, show, edit, rename, remove, repair, doctor, uninstall | Add, list, remove |

The variants use separate `%LOCALAPPDATA%` roots and distinct wrapper markers. They can be installed independently, but defining the same alias in both creates normal PATH-order ambiguity; install only the bin directory whose alias should win.

## Why Nalias exists

Shell-specific profile aliases are easy to lose, differ between CMD and PowerShell, and are often unavailable in newly opened shells. Nalias gives both shells the same user-scoped alias directory and keeps commands editable in a central configuration file.

## Installation

Download `nalias.exe`, then execute it or run:

```powershell
.\nalias.exe
```

Running `nalias.exe` without arguments—including double-clicking it in File Explorer—is equivalent to `nalias init --force`. It copies or updates the executable at `%LOCALAPPDATA%\Nalias\nalias.exe`, but first compares both files byte for byte and skips replacement when the installed build is already identical. It also creates the configuration and wrapper directory and adds `%LOCALAPPDATA%\Nalias\bin` to the current user's PATH. Administrator privileges are not required. Restart terminals that were already open. Use the explicit `init --skip-path` command when PATH should remain unchanged.

## Build and release

Install a current stable Rust toolchain, clone the repository, and run:

```powershell
cargo build
cargo build --release
```

The optimized standalone binary is `target\release\nalias.exe`. The release profile uses size optimization, LTO, one codegen unit, symbol stripping, and abort-on-panic.

## Quick start

```powershell
nalias init
nalias add gs "git status" --description "Show repository status"
gs --short --branch
nalias edit gs --command "git status --short"
nalias rename gs status
nalias remove status --yes
```

## Command reference

| Command | Purpose |
| --- | --- |
| `nalias` | Initialize Nalias and update the installed executable only when it differs. |
| `nalias init [--force] [--skip-path]` | Initialize and install Nalias. |
| `nalias add <name> <command> [--description <text>] [--shell cmd\|powershell\|direct] [--force]` | Add or explicitly replace an alias. |
| `nalias run <name> [--dry-run] [arguments...]` | Resolve and execute an alias. Usually called by wrappers. |
| `nalias list [--json]` | List aliases as a table or machine-readable JSON. |
| `nalias show <name> [--json]` | Show every stored field. |
| `nalias edit <name> [--command <text>] [--description <text>] [--shell <shell>] [--enable\|--disable]` | Change an alias without rewriting its wrapper. |
| `nalias rename <old> <new>` | Rename the JSON key and wrapper transactionally. |
| `nalias remove <name> [--yes]` | Confirm and remove an alias. |
| `nalias repair` | Reconcile generated wrappers and remove only marked stale wrappers. |
| `nalias doctor` | Check installation, configuration, PATH, wrappers, and executable consistency. |
| `nalias uninstall [--keep-config] [--yes]` | Remove generated assets and the exact Nalias PATH entry. |

All commands support normal Clap help, `nalias --help`, and `nalias --version`. `--verbose` prints the selected execution mode and invocation.

## Architecture

Definitions never get embedded into wrappers. Editing a command changes JSON only.

```text
User enters:
    gs --short

Windows resolves:
    %LOCALAPPDATA%\Nalias\bin\gs.cmd

Wrapper invokes:
    nalias.exe run gs --short

Nalias loads:
    %LOCALAPPDATA%\Nalias\aliases.json

Nalias resolves:
    gs → git status

Nalias executes:
    cmd.exe /D /S /C "git status --short"
```

The code separates CLI parsing, paths, configuration, alias validation, wrappers, execution, application commands, and Windows platform integration. This boundary leaves room for future Linux and macOS backends.

## Configuration

`%LOCALAPPDATA%\Nalias\aliases.json` is versioned and human-readable:

```json
{
  "version": 1,
  "aliases": {
    "gs": {
      "command": "git status",
      "description": "Show Git working tree status",
      "shell": "cmd",
      "enabled": true,
      "created_at": "2026-08-04T15:00:00Z",
      "updated_at": "2026-08-04T15:00:00Z"
    }
  }
}
```

Nalias validates the complete document and never resets malformed or unsupported configuration. Updates use a same-directory temporary file, durable flush, Windows `ReplaceFileW`, a `.bak` backup, and a lock spanning each read–modify–write transaction.

## PATH behavior

Only `HKEY_CURRENT_USER\Environment\Path` is changed. Entries are compared case-insensitively after slash and trailing-separator normalization. Existing entries and order are preserved, duplicate installation is avoided, and uninstall removes only the exact Nalias bin path. `WM_SETTINGCHANGE` is broadcast after a change. When `NALIAS_HOME` is set, registry changes are automatically disabled to keep tests and isolated installations safe.

## Argument forwarding and shells

- `cmd` (default) uses `cmd.exe /D /S /V:ON /C`, so operators in the stored command such as `&&`, `|`, and redirection work. Forwarded values travel in per-argument environment placeholders and expand only in CMD's late parsing phase, after shell metacharacters have been recognized.
- `powershell` uses `powershell.exe -NoLogo -NoProfile -Command` and single-quoted literal forwarded values.
- `direct` parses the stored command with Microsoft C command-line quoting rules, starts the program with `std::process::Command`, then passes forwarded arguments as native argument values. Use direct mode when exact arbitrary argument preservation matters and shell operators are not needed.

There is no universally lossless representation of every value through `cmd.exe` because its parser performs multiple expansion phases. Nalias enables delayed expansion for CMD aliases to safely carry spaces, quotes, ampersands, pipes, redirection characters, parentheses, percent signs, carets, and exclamation marks (expanded environment values are not rescanned). Literal `!` sequences in the stored command itself do participate in delayed expansion; use `direct` or `powershell` mode if that changes the intended command. Program-specific parsing of embedded quotes can also differ. The calling shell parses text before the wrapper receives it, so quote metacharacters according to that shell.

## Security

Aliases are commands the user intentionally authorizes. Listing and loading never execute them. Names are ASCII-only, cannot traverse paths, and reject Windows device names and `nalias`. Wrapper writes reject symlinks and unrelated files. Generated wrappers carry a marker, and repair/uninstall delete only marked wrappers. Forwarded shell arguments are escaped, recursion is stopped through `NALIAS_ALIAS_STACK`, and nesting is limited to 32. Use `nalias run <name> --dry-run` or global `--verbose` to inspect execution.

## Troubleshooting

- Run `nalias doctor`; a nonzero exit means an important installation problem was found.
- Restart terminals opened before `init`, because running processes keep their old environment.
- Run `nalias repair` after restoring or manually moving configuration.
- If another command has the same name, inspect `where.exe <name>` and user PATH order.
- Do not delete `aliases.lock` while Nalias is running. A lock timeout reports concurrent work instead of overwriting it. A process crash can leave a stale lock; after confirming no Nalias process is running, remove only `%LOCALAPPDATA%\Nalias\aliases.lock` and retry.
- A running installed executable cannot delete itself immediately on Windows. Uninstall launches a hidden, bounded cleanup script that removes it after the process exits, then removes the now-empty Nalias directory. Reboot-delayed deletion is the fallback if that helper cannot start.

## Development and testing

Rust 2024 edition and stable Rust are used. Run the full local checks with:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --release
cargo build --release -p nalias-lite
```

Tests create temporary homes through `NALIAS_HOME`; they do not edit the developer's real `%LOCALAPPDATA%` or registry PATH. The Windows integration tests exercise initialization, wrappers, execution, listing, showing, editing, renaming, removal, dry runs, recursion, and corrupt-config safety.

## Uninstall

```powershell
nalias uninstall --yes
# Preserve aliases.json instead:
nalias uninstall --keep-config --yes
```

Unrelated files in the bin or root directory are preserved. A retained unrelated file can keep its directory from being removed.

## Roadmap

- Native Linux and macOS wrapper/PATH backends
- Structured direct-mode executable and base-argument fields
- Optional import/export and shell completion
- Signed release automation

## License

Nalias is available under the [MIT License](LICENSE).
