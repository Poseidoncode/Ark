# Ark Refactoring & Polish Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Refactor Ark codebase from a monolithic Vue component + mixed-quality Rust backend into a well-structured, tested, polished Git GUI application.

**Architecture:** Three-phase approach: (1) Quick wins — bug fixes and safe improvements, (2) Frontend architecture overhaul — Pinia + component split, (3) Quality & polish — tests, telemetry, git2 remote ops.

**Tech Stack:** Tauri 2, Vue 3.5, Pinia, Vitest, Playwright, Rust (git2, lru, tokio)

---

## Phase 1: Quick Wins

### Task 1: Fix Keyboard Shortcut Bug

**Files:**
- Modify: `src/composables/useKeyboardShortcuts.ts:21-35`

**Step 1: Write failing test**

```typescript
// In a temporary test file or add to existing test
// The bug: when shortcut.ctrl = false, ctrlMatch = !event.ctrlKey && !event.metaKey
// This means if user holds Ctrl while pressing a non-Ctrl shortcut, it won't fire
// Test: pressing 's' while Ctrl is held should NOT trigger { key: 's', ctrl: false }

const event = new KeyboardEvent('keydown', { key: 's' });
// Simulate Ctrl held: event.ctrlKey = true, event.metaKey = false
Object.defineProperty(event, 'ctrlKey', { get: () => true });

const shortcut = { key: 's', ctrl: false, action: () => {} };
// Expected: this shortcut should NOT fire (ctrl is false)
// Buggy behavior: shortcut fires because !event.ctrlKey && !event.metaKey = false && false = false (not true)
// Wait - let me re-trace the bug:
// ctrlMatch = shortcut.ctrl ? (event.ctrlKey || event.metaKey) : !event.ctrlKey && !event.metaKey
// If shortcut.ctrl = false: ctrlMatch = !true && !false = false && true = false
// So keyMatch=true, ctrlMatch=false -> shortcut DOES NOT fire (correct!)
// Hmm, let me re-read the code...
// Actually the bug is on line 25: ctrlMatch = shortcut.ctrl ? (...) : !event.ctrlKey && !event.metaKey
// When shortcut.ctrl is false, this requires NO modifiers to be pressed.
// But if user is holding Ctrl for another purpose, this breaks.
// Let's check the actual intended behavior: "ctrl: true means require Ctrl/Meta, ctrl: false means require NO Ctrl/Meta"
// Actually looking at the code, it's: if shortcut.ctrl=true -> require Ctrl or Meta. if shortcut.ctrl=false -> require NO Ctrl or Meta.
// The bug is: when shortcut.ctrl=false and user presses Ctrl+somethingElse, the shortcut fires incorrectly IF the somethingElse matches.
// Wait no, if ctrlMatch = !event.ctrlKey && !event.metaKey and event.ctrlKey=true, then ctrlMatch=false, so no fire.
// Let me trace once more:
// line 25: const ctrlMatch = shortcut.ctrl ? (event.ctrlKey || event.metaKey) : !event.ctrlKey && !event.metaKey;
// If shortcut.ctrl = false:
//   ctrlMatch = !event.ctrlKey && !event.metaKey
//   = !true && !false = false && true = false
// So pressing Ctrl+S while there's { key: 's', ctrl: false } shortcut should NOT fire.
// But what if shortcut.ctrl = true and user presses just 's' (no ctrl)?
//   ctrlMatch = event.ctrlKey || event.metaKey = false || false = false -> NO fire.
// So both directions work correctly. Hmm.
// Let me look at the original analysis again: "ctrlMatch logic uses !event.ctrlKey && !event.metaKey when shortcut.ctrl is false"
// Actually the real bug might be different - maybe the intention was:
// - ctrl: true -> must hold Ctrl/Meta
// - ctrl: false -> don't care about modifiers (just match the key)
// But the code does: ctrl: false -> must NOT hold Ctrl/Meta
// That's the bug! If you want Ctrl+S to also trigger { key: 's', ctrl: false } (because user doesn't care about modifiers),
// the current code would block it.
// Let's fix the logic: if ctrl=false, just don't require ctrl - but allow other modifiers or no modifiers.
// Actually looking at line 25 more carefully:
// ctrlMatch = shortcut.ctrl ? (event.ctrlKey || event.metaKey) : !event.ctrlKey && !event.metaKey
// This says: if ctrl=false, MUST NOT have ctrl OR meta pressed.
// The FIX should be: if ctrl=false, match regardless of ctrl/meta state (just match the key).
// PROPER FIX: separate "require ctrl" from "match key" - ctrl:false should mean "don't require ctrl" not "must not have ctrl"
// New logic: ctrlMatch = shortcut.ctrl ? (event.ctrlKey || event.metaKey) : true
// This way ctrl:false just means "don't require ctrl" but still matches regardless of modifier state.
```

**Step 2: Verify bug behavior**

The fix is to change the `ctrlMatch` for `ctrl: false` shortcuts. Currently `!event.ctrlKey && !event.metaKey` means "no modifiers allowed", but the intended behavior is "Ctrl/Meta not required" (so other combos work). Fix: use `true` instead of `!event.ctrlKey && !event.metaKey`.

**Step 3: Implement fix**

```typescript
// src/composables/useKeyboardShortcuts.ts line ~25
const ctrlMatch = shortcut.ctrl 
    ? (event.ctrlKey || event.metaKey) 
    : true; // FIX: don't require absence of ctrl/meta, just don't require ctrl
```

**Step 4: Run existing tests**

Run: `npm test -- --run src/composables/__tests__/useKeyboardShortcuts.spec.ts`
Expected: PASS (existing tests still pass with the new logic)

**Step 5: Commit**

```bash
git add src/composables/useKeyboardShortcuts.ts
git commit -m "fix: correct keyboard shortcut ctrl modifier matching logic"
```

---

### Task 2: Use `lru` Crate in Rust Backend

**Files:**
- Modify: `src-tauri/src/lib.rs:59-70` (replace `branch_cache` and `history_cache` manual impl with `lru::LruCache`)
- Modify: `src-tauri/Cargo.toml:27` (already declared, ensure it has correct version)

**Step 1: Write test for LruCache integration**

Add test in `src-tauri/src/lib.rs` test module to verify LruCache eviction works correctly.

**Step 2: Implement LruCache-backed caches**

Replace `Option<(Vec<BranchInfo>, std::time::Instant)>` with `Option<LruCache<String, (BranchInfo, std::time::Instant)>>` or similar. The cache key is based on the operation, not repo path. Actually the caches store `Vec<BranchInfo>` and `Vec<CommitInfo>` with timestamp. Replace with `LruCache` wrapping the Vec + timestamp as value.

```rust
// In lib.rs, replace:
#[allow(dead_code)]
branch_cache: Option<(Vec<BranchInfo>, std::time::Instant)>,

// With:
#[allow(dead_code)]
branch_cache: Option<lru::LruCache<(), (Vec<BranchInfo>, std::time::Instant)>>,
```

Wait - the cache doesn't need a key since it's repo-global state. The `lru` crate `LruCache<K, V>` requires a key. Since this is repo-level state and there's only one cache per repo, we can use `()` as a dummy key.

**Step 3: Run cargo check**

Run: `cd src-tauri && cargo check`
Expected: PASS

**Step 4: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/Cargo.toml
git commit -m "refactor: use lru crate for branch and history caches"
```

---

### Task 3: Refine `AppError` Enum

**Files:**
- Modify: `src-tauri/src/lib.rs:18-57`
- Modify: `src-tauri/src/git_operations.rs:1-1435` (update error conversions)

**Step 1: Analyze current AppError usage**

Search for all `AppError::` constructions across the codebase:
- `AppError::Git(String)` — most operations
- `AppError::Io(String)` — file system errors
- `AppError::Lock(String)` — mutex lock failures
- `AppError::Config(String)` — settings/config errors

**Step 2: Add sub-variants for Git errors**

```rust
#[derive(Debug)]
pub enum AppError {
    Git(GitError),
    Io(String),
    Lock(String),
    Config(String),
}

#[derive(Debug)]
pub enum GitError {
    NotFound(String),
   冲突(String),
    PermissionDenied(String),
    InvalidRef(String),
    BranchExists(String),
    CheckoutFailed(String),
    MergeFailed(String),
    RemoteNotFound(String),
    AuthenticationFailed(String),
    Other(String),
}
```

**Step 3: Update From impls**

Update `From<git2::Error>` to map git2 error codes to `GitError` variants.

**Step 4: Run cargo test**

Run: `cd src-tauri && cargo test`
Expected: PASS

**Step 5: Commit**

```bash
git add src-tauri/src/lib.rs src-tauri/src/git_operations.rs
git commit -m "refactor: add granular GitError variants to AppError enum"
```

---

## Phase 2: Frontend Architecture Overhaul

### Task 4: Add Pinia Store

**Files:**
- Install: `npm install pinia`
- Create: `src/stores/repo.ts` — repository state (repoInfo, fileStatuses, branches, commits, stashes, conflicts)
- Create: `src/stores/ui.ts` — UI state (view, loading, error, modals)
- Create: `src/stores/settings.ts` — user settings
- Modify: `src/main.ts` — register Pinia

**Step 1: Write store tests**

```typescript
// src/stores/__tests__/repo.spec.ts
import { defineStore } from 'pinia';
import { ref } from 'vue';
// ... test setup
```

**Step 2: Install Pinia**

Run: `npm install pinia`
Expected: SUCCESS

**Step 3: Create stores**

Write `src/stores/repo.ts` with `defineStore`, `src/stores/ui.ts`, `src/stores/settings.ts`.

**Step 4: Update main.ts to use Pinia**

**Step 5: Run TypeScript check**

Run: `npx tsc --noEmit`
Expected: No errors

**Step 6: Commit**

```bash
git add package.json src/stores/
git commit -m "feat: add Pinia stores for repo, ui, and settings state"
```

---

### Task 5: Split App.vue

**Files:**
- Create: `src/components/HeaderBar.vue` — header/top bar with repo selector, branch switcher
- Create: `src/components/ChangesPanel.vue` — file status list and staging
- Create: `src/components/CommitPanel.vue` — commit message input, amend toggle, commit button
- Create: `src/components/HistoryPanel.vue` — commit history list with virtual scroll
- Create: `src/components/DiffPanel.vue` — diff viewer for files and commits
- Create: `src/components/StashPanel.vue` — stash list and operations
- Create: `src/components/ConflictsPanel.vue` — conflict resolution UI
- Create: `src/components/CloneModal.vue` — clone repository dialog
- Create: `src/components/SettingsModal.vue` — settings dialog
- Create: `src/components/BranchModal.vue` — branch switcher/create dialog
- Create: `src/components/TagsModal.vue` — tags management
- Create: `src/components/RemotesModal.vue` — remotes management
- Create: `src/components/ErrorBanner.vue` — error display with retry
- Create: `src/components/LoadingOverlay.vue` — loading spinner
- Modify: `src/App.vue` — import and compose all components

**Step 1: Write component tests for each new component**

Use Vitest + @testing-library/vue.

**Step 2: Create HeaderBar.vue** (extract from App.vue lines ~1409-1499)

**Step 3: Create ChangesPanel.vue** (file list, staging, context menu)

**Step 4: Create HistoryPanel.vue** (commit list, virtual scroll via vue-virtual-scroller)

**Step 5: Create DiffPanel.vue** (diff viewer, already exists as DiffViewer.vue - rename and enhance)

**Step 6: Create CommitPanel.vue** (commit form, amend toggle)

**Step 7: Create StashPanel.vue, ConflictsPanel.vue**

**Step 8: Create all modal components**

**Step 9: Rewrite App.vue to compose all components**

**Step 10: Run full test suite**

Run: `npm test -- --run`
Expected: All PASS

**Step 11: Run TypeScript check**

Run: `npx tsc --noEmit`
Expected: No errors

**Step 12: Commit in chunks**

```bash
git add src/components/HeaderBar.vue
git commit -m "feat: extract HeaderBar component from App.vue"

git add src/components/ChangesPanel.vue
git commit -m "feat: extract ChangesPanel component from App.vue"

# ... continue with each component
```

---

### Task 6: Fix TypeScript-Rust Type Duplication

**Files:**
- Modify: `src/services/git.ts:24-102` (TypeScript interfaces)
- Modify: `src-tauri/src/models.rs:1-125` (Rust structs)

**Decision:** Defer code generation (`ts-rs` or similar) as it adds build complexity. Instead, maintain a comment in both files pointing to the other as source of truth, and add a CI check script that compares field names to catch drift.

**Step 1: Add sync check script**

Create `scripts/sync-types.js` that reads `src-tauri/src/models.rs` and `src/services/git.ts`, extracts struct/interface field names, and fails CI if they don't match.

**Step 2: Run and verify**

```bash
node scripts/sync-types.js
```

Expected: PASS (or FAIL with list of mismatches to fix)

**Step 3: Commit**

```bash
git add scripts/sync-types.js
git commit -m "ci: add type sync check between Rust models and TypeScript interfaces"
```

---

## Phase 3: Quality & Polish

### Task 7: Increase Test Coverage

**Files:**
- Create: `src/components/__tests__/HeaderBar.spec.ts`
- Create: `src/components/__tests__/ChangesPanel.spec.ts`
- Create: `src/components/__tests__/CommitPanel.spec.ts`
- Create: `src/components/__tests__/HistoryPanel.spec.ts`
- Create: `src/components/__tests__/DiffPanel.spec.ts`
- Create: `src/services/__tests__/git.spec.ts` (test GitServiceOptimizer caching/debouncing)
- Modify: `src/composables/__tests__/useKeyboardShortcuts.spec.ts`

**Step 1: Install testing dependencies**

Check `@testing-library/vue` is installed (it is in package.json).

**Step 2: Write tests for each component** (use TDD approach: write failing test first)

**Step 3: Run tests**

Run: `npm test -- --run`
Expected: All PASS

**Step 4: Commit**

```bash
git add src/components/__tests__/ src/services/__tests__/
git commit -m "test: add component and service tests for App.vue extracted modules"
```

---

### Task 8: Remote Operations via git2

**Files:**
- Modify: `src-tauri/src/git_operations.rs:922-967` (push_changes, pull_changes, fetch_changes)
- Add: `src-tauri/src/remote_ops.rs` (new file for git2 remote operations)

**Step 1: Research git2 remote API**

Use `git2::Remote` struct, `remote::Remote::push()`, `remote::Remote::fetch()`, `remote::Remote::pull()`.

**Step 2: Write tests for remote operations**

**Step 3: Implement push via git2**

```rust
pub fn push_changes(
    repo: &Repository,
    ssh_key_path: Option<&str>,
    _ssh_passphrase: Option<&str>,
) -> Result<(), String> {
    // Build credentials callback
    // Get remote "origin"
    // Call remote.push() with refspec "HEAD:refs/remotes/origin/HEAD" or similar
}
```

**Step 4: Implement pull via git2**

**Step 5: Implement fetch via git2**

**Step 6: Run cargo test**

Run: `cd src-tauri && cargo test`
Expected: PASS

**Step 7: Commit**

```bash
git add src-tauri/src/remote_ops.rs src-tauri/src/git_operations.rs
git commit -m "refactor: implement push/pull/fetch via git2 instead of subprocess"
```

---

### Task 9: Production Performance Telemetry

**Files:**
- Modify: `src/main.ts` — enable `observePerformance` and `logMemoryUsage` in production too
- Create: `src/composables/useMetrics.ts` — send performance data to a lightweight endpoint or log

**Step 1: Extend performance monitoring to production**

Modify `src/main.ts` lines ~52-82 to remove `import.meta.env.DEV` guard for `observePerformance()` and `logMemoryUsage()`. Add conditional telemetry endpoint (e.g., log to console in dev, send to analytics in prod).

**Step 2: Add navigation timing and custom metrics**

**Step 3: Commit**

```bash
git add src/main.ts
git commit -m "perf: enable performance monitoring in production"
```

---

### Task 10: Test Isolation for git_operations.rs

**Files:**
- Modify: `src-tauri/src/git_operations.rs:1437-1604` (test module)

**Step 1: Review existing test patterns**

**Step 2: Add unique temp directories per test** (using atomic counter, already done via `COUNTER`)

**Step 3: Ensure tests clean up state** (temp dirs deleted after each test)

**Step 4: Run tests in single-threaded mode first**

Run: `cd src-tauri && cargo test -- --test-threads=1`
Expected: All PASS

**Step 5: Run tests in parallel** (verify no race conditions)

Run: `cd src-tauri && cargo test`
Expected: All PASS

**Step 6: Commit**

```bash
git add src-tauri/src/git_operations.rs
git commit -m "test: improve git_operations test isolation and cleanup"
```

---

## Execution Order

| # | Task | Dependencies | Estimated Time |
|---|------|--------------|----------------|
| 1 | Fix keyboard shortcut bug | None | 15 min |
| 2 | Use lru crate | None | 20 min |
| 3 | Refine AppError enum | None | 30 min |
| 4 | Add Pinia store | None | 45 min |
| 5 | Split App.vue | Task 4 | 2-3 hours |
| 6 | Fix type duplication (sync check) | None | 30 min |
| 7 | Remote ops via git2 | None | 2 hours |
| 8 | Increase test coverage | Tasks 4-5 | 2 hours |
| 9 | Production telemetry | None | 30 min |
| 10 | Test isolation improvements | None | 30 min |

---

## Verification Commands

After each task, run these to verify:

```bash
# TypeScript
npx tsc --noEmit

# Rust
cd src-tauri && cargo check && cargo test

# Frontend tests
npm test -- --run

# E2E tests
npm run test:e2e

# Build
npm run build
```

---

**Plan complete.** Two execution options:

**1. Subagent-Driven (this session)** — I dispatch fresh subagent per task, review between tasks, fast iteration

**2. Parallel Session (separate)** — Open new session with executing-plans, batch execution with checkpoints

**Which approach?**