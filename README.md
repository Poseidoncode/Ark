# Ark ⛴️

A modern, lightweight, and intuitive Git GUI client inspired by GitHub Desktop.

**Ark** is designed to be the safe harbor for your codebase. It abstracts away the complexity of Git commands into a clean, visual interface, allowing developers to focus entirely on building great products rather than wrestling with version control.

## 🚀 Quick Install

The one-line script builds **Ark** from source on your machine.
It downloads dependencies, compiles Rust, and packages a native bundle. The first
release build can take many minutes, especially during compilation and linking.
The script labels five stages: source download, npm dependencies, frontend build,
Rust compilation, and installer packaging. npm download activity and verbose
Tauri build output are shown live.
Each stage prints its elapsed time every 30 seconds; this indicates that the
command is still running, not that it is necessarily making progress.

### macOS / Linux

```sh
curl -fsSL https://raw.githubusercontent.com/Poseidoncode/Ark/main/install.sh | sh
```

### Windows (PowerShell)

```powershell
curl.exe -fsSL https://raw.githubusercontent.com/Poseidoncode/Ark/main/install.sh | sh
```

> **Note:** On Windows, run the command above in **Git Bash** or **WSL**,
> which provide the `sh` and `curl` utilities. Windows 10/11 ships with
> `curl.exe` built-in — use `curl.exe` (not `curl`) to avoid aliasing to
> `Invoke-WebRequest`.

If a build fails or is interrupted, use the printed `cd ... && sh install.sh`
command to retry in the same directory and reuse the Rust compilation cache.
Running the curl command again outside that directory creates a fresh checkout.
The build directory is kept after completion so the installer remains available.

If Cargo prints `Blocking waiting for file lock on artifact directory`, another
build is using the same output directory. `Ctrl+Z` suspends a build and can leave
its lock held. In the original terminal, use `jobs -l` to identify suspended jobs
and `fg %N` to resume one (replace `N` with its job number). Use `Ctrl+C` to cancel
duplicate builds; keep only one build running. Do not delete the Cargo lock file.

### Prerequisites

The install script requires the following tools to be installed beforehand:

| Tool | macOS | Windows | Linux |
|------|-------|---------|-------|
| [Node.js](https://nodejs.org) 22.22.2+ (22.x), 24.15.0+ (24.x), or ≥ 26 | ✅ | ✅ | ✅ |
| [Rust](https://rustup.rs) (stable) | ✅ | ✅ | ✅ |
| [Git](https://git-scm.com) | ✅ | ✅ | ✅ |

The script will check for these and exit early with a helpful message if
any are missing.

### ✨ Key Features

* **Frictionless Workflow:** One-click fetch, commit, and push operations.
* **Visual History & Diffs:** Crystal-clear code comparisons and timeline tracking.
* **Painless Branching:** Confidently create, switch, and merge branches without CLI anxiety.
