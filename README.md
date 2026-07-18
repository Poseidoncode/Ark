# Ark ⛴️

A modern, lightweight, and intuitive Git GUI client inspired by GitHub Desktop.

**Ark** is designed to be the safe harbor for your codebase. It abstracts away the complexity of Git commands into a clean, visual interface, allowing developers to focus entirely on building great products rather than wrestling with version control.

## 🚀 Quick Install

The fastest way to install **Ark** is via our one-line install script.
It auto-detects your operating system and builds a native bundle for you.

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

### Prerequisites

The install script requires the following tools to be installed beforehand:

| Tool | macOS | Windows | Linux |
|------|-------|---------|-------|
| [Node.js](https://nodejs.org) ≥ 18 | ✅ | ✅ | ✅ |
| [Rust](https://rustup.rs) (stable) | ✅ | ✅ | ✅ |
| [Git](https://git-scm.com) | ✅ | ✅ | ✅ |

The script will check for these and exit early with a helpful message if
any are missing.

### ✨ Key Features

* **Frictionless Workflow:** One-click fetch, commit, and push operations.
* **Visual History & Diffs:** Crystal-clear code comparisons and timeline tracking.
* **Painless Branching:** Confidently create, switch, and merge branches without CLI anxiety.
