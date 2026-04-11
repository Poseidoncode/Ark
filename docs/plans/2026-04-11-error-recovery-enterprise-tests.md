# Ark Error Recovery Enterprise Tests Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add and run enterprise-grade Rust tests that verify Ark's Git error recovery behavior around safety refs, conflicted operations, cleanup paths, and boundary failures, while documenting gaps where recovery mechanisms are missing.

**Architecture:** Extend the existing in-file Rust unit tests in `src-tauri/src/git_operations.rs` for low-level Git workflows and add focused unit tests in `src-tauri/src/lib.rs` for app-layer error mapping helpers. Use temporary repositories and real Git operations to verify recovery behavior instead of mocks, and capture unsupported/error-recovery gaps explicitly when no implementation exists (for example `lastFailedOperation`).

**Tech Stack:** Rust, git2, Tauri command layer, cargo test, LSP diagnostics

---

### Task 1: Inspect current recovery code and codify gaps

**Files:**
- Modify: `src-tauri/src/git_operations.rs`
- Modify: `src-tauri/src/lib.rs`
- Create: `docs/plans/2026-04-11-error-recovery-enterprise-tests.md`

**Step 1: Read the existing recovery-related code**

Inspect `create_safety_ref`, `amend_last_commit`, `cherry_pick`, `revert_commit`, clone/open error paths, and app-layer `No repository open` handling.

**Step 2: Record unsupported scope explicitly**

Confirm whether `lastFailedOperation` exists; if absent, treat it as an identified gap to report rather than inventing unrequested production behavior.

**Step 3: Define test targets**

Target: safety ref creation, manual restore viability, conflict-state persistence before cleanup, post-success cleanup, invalid path errors, and app-layer no-repo error mapping.

**Step 4: Run/confirm baseline failing tests for new behaviors**

Run targeted cargo tests for not-yet-implemented test names after adding them.

### Task 2: Add safety-ref and Git recovery regression tests

**Files:**
- Modify: `src-tauri/src/git_operations.rs`
- Test: `src-tauri/src/git_operations.rs`

**Step 1: Write failing tests for safety refs**

Add tests that verify:
- `amend_last_commit` creates `refs/safety/amend/*`
- `cherry_pick` creates `refs/safety/cherry-pick/*`
- `revert_commit` creates `refs/safety/revert/*`
- a saved safety ref can be used to reset the repository manually back to the original commit

**Step 2: Run targeted tests to verify RED**

Run the new tests and confirm any failures are due to missing assertions/helpers, not setup bugs.

**Step 3: Add minimal supporting helpers if tests need them**

If repeated repository/test setup makes recovery assertions unclear, extract tiny test helpers only inside the test module.

**Step 4: Re-run targeted tests to verify GREEN**

Run the new safety-ref tests until they pass.

### Task 3: Add conflict/cleanup and boundary recovery tests

**Files:**
- Modify: `src-tauri/src/git_operations.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: `src-tauri/src/git_operations.rs`
- Test: `src-tauri/src/lib.rs`

**Step 1: Write failing tests for recovery/error paths**

Add tests for:
- conflicted `cherry_pick` leaves repository in cherry-pick state and preserves conflict markers for manual recovery
- conflicted `revert_commit` leaves repository in revert state and preserves conflict markers for manual recovery
- failed clone with invalid URL/path leaves destination unopened/clean enough to report failure deterministically
- invalid path open returns the expected cleanup-facing error message
- app-layer helper returns `No repository open` when no repo is loaded

**Step 2: Run targeted tests to verify RED**

Run just the new tests.

**Step 3: Write minimal implementation/refactor**

Only if needed, add a tiny helper in `lib.rs` for app-layer repository lookup so the no-repo error path is directly testable without changing behavior.

**Step 4: Re-run targeted tests to verify GREEN**

Run the targeted recovery/error tests until they pass.

### Task 4: Verify diagnostics and execute the focused suite

**Files:**
- Modify: `src-tauri/src/git_operations.rs`
- Modify: `src-tauri/src/lib.rs`

**Step 1: Run LSP diagnostics**

Run diagnostics on changed Rust files and fix all reported errors/warnings relevant to the changes.

**Step 2: Run focused cargo test commands**

Run `cargo test` for the new error recovery tests in `src-tauri`.

**Step 3: Capture residual product gaps**

Report missing `lastFailedOperation`, missing clone-cleanup state management, or any other unimplemented enterprise recovery behavior discovered by the tests.

**Step 4: Stop after first successful verification**

Do not re-run verification once diagnostics and focused tests are clean.
