# Nalias Lite

Nalias Lite is an intentionally minimal companion to Nalias. It creates direct `.cmd` wrappers and does not participate when an alias runs.

```text
nalias-lite add gs "git status"
               |
               +-- creates bin\gs.cmd containing: git status %*

gs --short ----+-- Windows executes the wrapper directly
```

## Build

From the repository root:

```powershell
cargo build --release -p nalias-lite
```

The executable is written to `target\release\nalias-lite.exe`.

## Install

Run or double-click `nalias-lite.exe`. A no-argument launch is equivalent to `init --force` and installs to:

```text
%LOCALAPPDATA%\NaliasLite\nalias-lite.exe
%LOCALAPPDATA%\NaliasLite\bin\
```

The bin directory is added to the current user's PATH without administrator privileges. Restart already-open terminals.

## Commands

```text
nalias-lite init [--force] [--skip-path]
nalias-lite add <name> <command> [--force]
nalias-lite list
nalias-lite remove <name> [--yes]
nalias-lite help
```

Examples:

```powershell
nalias-lite add gs "git status"
gs --short
nalias-lite list
nalias-lite add gs "git status --short" --force
nalias-lite remove gs --yes
```

## Storage and safety

There is no JSON configuration. Each generated wrapper is authoritative and contains:

- A distinct Nalias Lite marker
- A hexadecimal metadata comment used by `list`
- The command followed by `%*` for argument forwarding

Commands must fit on one line and use CMD syntax. Alias names are validated against traversal and Windows device names. Existing unrelated wrappers are never overwritten or removed. `NALIAS_LITE_HOME` overrides the installation root and disables registry PATH changes for testing.

## Deliberate limitations

Nalias Lite has no descriptions, timestamps, shell selection, enable/disable state, JSON backup, runtime recursion protection, doctor, repair, rename, or centralized configuration. Editing is performed with `add --force`. Because commands are embedded in wrappers, changing one requires rewriting that wrapper.

Use full Nalias when centralized JSON, robust shell dispatch, runtime validation, or richer management is more important than minimum executable size.

