# Security Policy

## Reporting a Vulnerability

If you discover a security vulnerability in Ark, please report it privately via **GitHub's Security Advisory**:

<https://github.com/Poseidoncode/Ark/security/advisories/new>

We will acknowledge your report within **48 hours** and work on a fix before public disclosure.

Please **do not** open a public issue for security vulnerabilities.

## Scope

- The Rust backend (`src-tauri/`)
- The Vue/TypeScript frontend (`src/`)
- Build and CI pipelines (`.github/`, `Makefile`)

Dependency vulnerabilities are automatically tracked via **Dependabot** and **CodeQL** scanning.
