# Security Fixes: SSH Passphrase, Path Traversal, Conflict Resolution

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fix 3 high-severity security vulnerabilities in the Ark Git GUI application.

**Architecture:** The fixes are independent and can be implemented sequentially. Path traversal fix is a pure Rust change with a new validation function applied to 3 existing functions. Conflict resolution fix replaces a stub with real git2 logic using index conflict entries. SSH passphrase fix removes plaintext storage from the Settings model and replaces it with OS keychain storage via the `keyring` crate, touching both Rust backend and TypeScript frontend.

**Tech Stack:** Rust, Tauri 2.3, git2 0.20, keyring 3.x, Vue 3, TypeScript

---

## Priority & Dependency Order

| Priority | Issue | Risk | Dependencies |
|----------|-------|------|-------------|
| P0 | Path Traversal (Issue 2) | File deletion outside repo | None |
| P1 | Fake resolve_conflict (Issue 3) | Data loss / silent failure | None |
| P2 | SSH Passphrase Plaintext (Issue 1) | Credential exposure | New crate: `keyring` |

Issues 2 and 3 are independent. Issue 1 is independent but has more moving parts.

## Atomic Commit Strategy

| Commit | Scope | Message |
|--------|-------|---------|
| 1 | Issue 2 | `security: add validate_repo_path and apply to stage/unstage/discard` |
| 2 | Issue 3 | `security: implement real ours/theirs conflict resolution` |
| 3 | Issue 1 | `security: replace plaintext ssh_passphrase with OS keychain storage` |

---

## Task 1: Path Traversal Prevention (Issue 2)

**Files:**
- Modify: `src-tauri/src/git_operations.rs:235-296` (stage_files, unstage_files)
- Modify: `src-tauri/src/git_operations.rs:440-460` (discard_changes)
- Test: `src-tauri/src/git_operations.rs` (existing test module starting at line 948)

### Step 1: Write failing tests for `validate_repo_path`

Add these tests inside the existing `mod tests` block (after line 1123 in `git_operations.rs`):

```rust
#[test]
fn test_validate_repo_path_rejects_absolute_path() {
    let (root, repo, _) = create_committed_repo();
    let workdir = repo.workdir().unwrap();
    let result = validate_repo_path(workdir, "/etc/passwd");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("must be relative"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn test_validate_repo_path_rejects_dot_dot_traversal() {
    let (root, repo, _) = create_committed_repo();
    let workdir = repo.workdir().unwrap();
    let result = validate_repo_path(workdir, "../../../etc/passwd");
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("path traversal"));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn test_validate_repo_path_rejects_embedded_dot_dot() {
    let (root, repo, _) = create_committed_repo();
    let workdir = repo.workdir().unwrap();
    let result = validate_repo_path(workdir, "subdir/../../etc/passwd");
    assert!(result.is_err());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn test_validate_repo_path_accepts_valid_relative_path() {
    let (root, repo, _) = create_committed_repo();
    let workdir = repo.workdir().unwrap();
    // Create the file so canonicalize works
    fs::write(root.join("valid.txt"), "ok").unwrap();
    let result = validate_repo_path(workdir, "valid.txt");
    assert!(result.is_ok());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn test_validate_repo_path_accepts_nested_relative_path() {
    let (root, repo, _) = create_committed_repo();
    let workdir = repo.workdir().unwrap();
    fs::create_dir_all(root.join("src/utils")).unwrap();
    fs::write(root.join("src/utils/helper.rs"), "ok").unwrap();
    let result = validate_repo_path(workdir, "src/utils/helper.rs");
    assert!(result.is_ok());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn test_discard_changes_rejects_path_traversal() {
    let root = get_temp_dir();
    let repo_path = root.join("repo");
    let outside_file = root.join("secret.txt");
    let repo = init_working_repo(&repo_path);
    commit_file(&repo_path, "tracked.txt", "tracked", "Initial commit");
    fs::write(&outside_file, "classified").unwrap();

    let result = discard_changes(&repo, "../secret.txt");
    assert!(result.is_err(), "path traversal must be rejected");
    assert_eq!(fs::read_to_string(&outside_file).unwrap(), "classified");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn test_stage_files_rejects_dot_dot_path() {
    let root = get_temp_dir();
    let repo_path = root.join("repo");
    let outside_file = root.join("secret.txt");
    fs::create_dir_all(&repo_path).unwrap();
    fs::write(&outside_file, "classified").unwrap();
    let repo = init_working_repo(&repo_path);
    commit_file(&repo_path, "tracked.txt", "tracked", "Initial commit");

    let result = stage_files(&repo, vec!["../secret.txt".to_string()]).unwrap();
    assert!(result.staged.is_empty(), "path traversal must not stage files");

    let _ = fs::remove_dir_all(root);
}
```

### Step 2: Run tests to verify they fail

Run: `cargo test --lib -- test_validate_repo_path test_discard_changes_rejects_path_traversal test_stage_files_rejects_dot_dot_path`
Working dir: `src-tauri`
Expected: Compilation error — `validate_repo_path` does not exist yet; `discard_changes("../secret.txt")` currently succeeds instead of returning `Err`.

### Step 3: Implement `validate_repo_path` function

Add this function near the top of `git_operations.rs` (after `is_safe_git_arg`, around line 70):

```rust
/// Validate that a user-supplied path is safe to use within a repository.
/// Returns the canonicalized full path on success.
/// Rejects: absolute paths, `..` components, paths resolving outside workdir.
fn validate_repo_path(workdir: &Path, path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);

    // Reject absolute paths
    if p.is_absolute() {
        return Err(format!("Path must be relative to the repository: {}", path));
    }

    // Reject any component that is ".."
    for component in p.components() {
        if let std::path::Component::ParentDir = component {
            return Err(format!("Illegal path traversal in: {}", path));
        }
    }

    // Canonicalize workdir and check the resolved path is within it
    let canonical_workdir = workdir
        .canonicalize()
        .map_err(|e| format!("Failed to resolve workdir: {}", e))?;

    let full_path = workdir.join(path);

    // For files that exist, canonicalize and check prefix
    if full_path.exists() {
        let canonical_full = full_path
            .canonicalize()
            .map_err(|e| format!("Failed to resolve path: {}", e))?;
        if !canonical_full.starts_with(&canonical_workdir) {
            return Err(format!("Path escapes repository boundary: {}", path));
        }
        Ok(canonical_full)
    } else {
        // For non-existent files (e.g., deleted files being staged),
        // the component check above is sufficient since we've already
        // rejected ".." and absolute paths.
        Ok(full_path)
    }
}
```

### Step 4: Apply `validate_repo_path` to `discard_changes`

Replace `discard_changes` function (lines 440-460):

```rust
pub fn discard_changes(repo: &Repository, path: &str) -> Result<(), String> {
    let workdir = repo.workdir().ok_or("No working directory found")?;
    validate_repo_path(workdir, path)?;

    let mut checkout_opts = git2::build::CheckoutBuilder::new();
    checkout_opts.force().path(path);

    // Attempt checkout from HEAD
    if repo.checkout_head(Some(&mut checkout_opts)).is_err() {
        // If checkout head fails (e.g. untracked file), try to remove it
        let full_path = workdir.join(path);
        if full_path.exists() {
            if full_path.is_file() {
                std::fs::remove_file(full_path)
                    .map_err(|e| format!("Failed to delete file: {}", e))?;
            } else if full_path.is_dir() {
                std::fs::remove_dir_all(full_path)
                    .map_err(|e| format!("Failed to delete dir: {}", e))?;
            }
        }
    }

    Ok(())
}
```

### Step 5: Apply `validate_repo_path` to `stage_files`

Modify the `for path in paths` loop in `stage_files` (line 244). Add validation at the start of the loop body:

```rust
for path in paths {
    if let Err(e) = validate_repo_path(workdir, &path) {
        warnings.push(format!("Rejected '{}': {}", path, e));
        continue;
    }
    let full_path = workdir.join(&path);
    if full_path.exists() {
        match index.add_path(Path::new(&path)) {
            Ok(_) => staged.push(path),
            Err(e) => warnings.push(format!("Failed to stage '{}': {}", path, e)),
        }
    } else {
        let _ = index.remove_path(Path::new(&path));
        warnings.push(format!(
            "Skipped '{}': file not found (removed from index)",
            path
        ));
    }
}
```

### Step 6: Apply `validate_repo_path` to `unstage_files`

Add validation at the start of `unstage_files` (after line 268). Filter out invalid paths before processing:

```rust
pub fn unstage_files(repo: &Repository, paths: Vec<String>) -> Result<(), String> {
    let workdir = repo.workdir().ok_or("No working directory found")?;
    let paths: Vec<String> = paths
        .into_iter()
        .filter(|p| validate_repo_path(workdir, p).is_ok())
        .collect();

    let head = repo.head().ok();
    // ... rest of existing implementation unchanged
```

### Step 7: Update existing test expectations

The existing test `test_discard_changes_ignores_absolute_path_outside_repo` (line 1125) currently expects `discard_changes` to succeed (`.unwrap()`). Update it to expect an error:

```rust
#[test]
fn test_discard_changes_ignores_absolute_path_outside_repo() {
    let root = get_temp_dir();
    let repo_path = root.join("repo");
    let outside_path = root.join("outside.txt");
    let repo = init_working_repo(&repo_path);
    commit_file(&repo_path, "tracked.txt", "tracked", "Initial commit");
    fs::write(&outside_path, "top-secret").unwrap();

    let result = discard_changes(&repo, outside_path.to_str().unwrap());
    assert!(result.is_err(), "absolute paths must be rejected");

    assert!(
        outside_path.exists(),
        "absolute paths should not delete files outside the repository"
    );
    assert_eq!(fs::read_to_string(&outside_path).unwrap(), "top-secret");

    let _ = fs::remove_dir_all(root);
}
```

### Step 8: Run all tests to verify they pass

Run: `cargo test --lib`
Working dir: `src-tauri`
Expected: ALL tests pass, including the new path traversal tests and the updated existing test.

### Step 9: Commit

```bash
git add src-tauri/src/git_operations.rs
git commit -m "security: add validate_repo_path and apply to stage/unstage/discard

Add path validation that rejects absolute paths, '..' traversal components,
and paths resolving outside the repository workdir via canonicalization.
Applied to stage_files, unstage_files, and discard_changes to prevent
path traversal attacks that could read/modify/delete files outside the repo."
```

---

## Task 2: Real Conflict Resolution (Issue 3)

**Files:**
- Modify: `src-tauri/src/git_operations.rs:897-910` (resolve_conflict)
- Test: `src-tauri/src/git_operations.rs` (existing test module)

### Step 1: Write failing tests for ours/theirs resolution

Add tests in the existing `mod tests` block. Place after the existing `test_conflict_detection_and_resolution_workflow` test:

```rust
#[test]
fn test_resolve_conflict_use_ours_writes_our_version() {
    let (root, repo, _) = create_merge_conflict_repo();

    let conflicts = get_conflicts(&repo).unwrap();
    assert_eq!(conflicts.len(), 1);

    // Resolve using "ours" (the main branch version)
    resolve_conflict(&repo, "base.txt", true).unwrap();

    let content = fs::read_to_string(root.join("base.txt")).unwrap();
    assert_eq!(content, "main version\n", "ours should be the main branch version");
    assert!(!repo.index().unwrap().has_conflicts());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn test_resolve_conflict_use_theirs_writes_their_version() {
    let (root, repo, _) = create_merge_conflict_repo();

    let conflicts = get_conflicts(&repo).unwrap();
    assert_eq!(conflicts.len(), 1);

    // Resolve using "theirs" (the feature branch version)
    resolve_conflict(&repo, "base.txt", false).unwrap();

    let content = fs::read_to_string(root.join("base.txt")).unwrap();
    assert_eq!(content, "feature version\n", "theirs should be the feature branch version");
    assert!(!repo.index().unwrap().has_conflicts());

    let _ = fs::remove_dir_all(root);
}
```

### Step 2: Run tests to verify they fail

Run: `cargo test --lib -- test_resolve_conflict_use_ours test_resolve_conflict_use_theirs`
Working dir: `src-tauri`
Expected: FAIL — the current implementation ignores `_use_ours` and stages whatever is on disk (the conflict markers), so the content assertions will fail.

### Step 3: Implement real `resolve_conflict`

Replace the `resolve_conflict` function (lines 897-910) with:

```rust
pub fn resolve_conflict(repo: &Repository, path: &str, use_ours: bool) -> Result<(), String> {
    let workdir = repo.workdir().ok_or("No working directory found")?;
    let mut index = repo
        .index()
        .map_err(|e| format!("Failed to get index: {}", e))?;

    // Find the conflict entry for this path
    let conflict = index
        .conflicts()
        .map_err(|e| format!("Failed to get conflicts: {}", e))?
        .find(|c| {
            if let Ok(c) = c {
                let conflict_path = c
                    .ancestor
                    .as_ref()
                    .or(c.our.as_ref())
                    .or(c.their.as_ref())
                    .map(|e| String::from_utf8_lossy(&e.path).to_string());
                conflict_path.as_deref() == Some(path)
            } else {
                false
            }
        });

    let conflict = conflict
        .ok_or_else(|| format!("No conflict found for path: {}", path))?
        .map_err(|e| format!("Conflict entry error: {}", e))?;

    // Pick the appropriate side's index entry
    let entry = if use_ours {
        conflict.our.ok_or_else(|| format!("No 'ours' entry for {}", path))?
    } else {
        conflict.their.ok_or_else(|| format!("No 'theirs' entry for {}", path))?
    };

    // Read the blob content from the chosen side
    let blob = repo
        .find_blob(entry.id)
        .map_err(|e| format!("Failed to read blob: {}", e))?;

    // Write the chosen content to the working directory
    let full_path = workdir.join(path);
    std::fs::write(&full_path, blob.content())
        .map_err(|e| format!("Failed to write resolved file: {}", e))?;

    // Remove the conflict and stage the resolved file
    index
        .remove_path(Path::new(path))
        .map_err(|e| format!("Failed to remove conflict entry: {}", e))?;
    index
        .add_path(Path::new(path))
        .map_err(|e| format!("Failed to stage resolved file: {}", e))?;
    index
        .write()
        .map_err(|e| format!("Failed to write index: {}", e))?;

    Ok(())
}
```

### Step 4: Update existing `test_conflict_detection_and_resolution_workflow`

The existing test (line 2179) manually writes "resolved version\n" to the file before calling `resolve_conflict`. This test calls `resolve_conflict(&repo, "base.txt", true)` which will now write "ours" content, overwriting the manual write. Update the test to match the new behavior:

```rust
#[test]
fn test_conflict_detection_and_resolution_workflow() {
    let (root, repo, _) = create_merge_conflict_repo();

    let conflicts = get_conflicts(&repo).unwrap();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].path, "base.txt");
    assert_eq!(conflicts[0].our_status, "modified");
    assert_eq!(conflicts[0].their_status, "modified");

    // Resolve using ours — now actually writes the "ours" content
    resolve_conflict(&repo, "base.txt", true).unwrap();

    assert!(get_conflicts(&repo).unwrap().is_empty());
    assert!(!repo.index().unwrap().has_conflicts());
    // File should contain "ours" (main branch) version
    let content = fs::read_to_string(root.join("base.txt")).unwrap();
    assert_eq!(content, "main version\n");

    let _ = fs::remove_dir_all(root);
}
```

### Step 5: Run all tests to verify they pass

Run: `cargo test --lib`
Working dir: `src-tauri`
Expected: ALL tests pass including the new ours/theirs tests and the updated workflow test.

### Step 6: Commit

```bash
git add src-tauri/src/git_operations.rs
git commit -m "security: implement real ours/theirs conflict resolution

Replace the stub resolve_conflict that ignored use_ours parameter.
Now reads the conflict entries from the git index, extracts the blob
for the chosen side (ours/theirs), writes it to the working directory,
removes the conflict markers from the index, and stages the resolved file."
```

---

## Task 3: SSH Passphrase Secure Storage (Issue 1)

**Files:**
- Modify: `src-tauri/Cargo.toml` (add keyring dependency)
- Create: `src-tauri/src/credential_store.rs` (keyring wrapper module)
- Modify: `src-tauri/src/models.rs:59-68` (remove ssh_passphrase from Settings)
- Modify: `src-tauri/src/lib.rs:114-140,186-190,310-316,331-337,352-358,436-452,555-614` (multiple sites)
- Modify: `src/services/git.ts:81-89` (remove ssh_passphrase from TS interface)
- Modify: `src/services/git.ts` (add new invoke wrappers for passphrase get/set/delete)

### Step 1: Add `keyring` dependency to Cargo.toml

Add to `[dependencies]` in `src-tauri/Cargo.toml`:

```toml
keyring = { version = "3", features = ["apple-native", "windows-native", "sync-secret-service"] }
```

Note: `apple-native` = macOS Keychain, `windows-native` = Windows Credential Store, `sync-secret-service` = Linux Secret Service (DBus). Verify feature names compile; if `sync-secret-service` fails, try `linux-native` or check `docs.rs/keyring/3` for exact feature names during execution.

### Step 2: Run cargo check to verify dependency resolves

Run: `cargo check`
Working dir: `src-tauri`
Expected: Compiles successfully (no code changes yet, just dependency addition).

### Step 3: Create `credential_store.rs` module

Create `src-tauri/src/credential_store.rs`:

```rust
//! Secure credential storage using OS-level keychain/credential store.
//! macOS: Keychain, Windows: Credential Manager, Linux: Secret Service

use keyring::Entry;

const SERVICE_NAME: &str = "com.ark.git-gui";
const SSH_PASSPHRASE_KEY: &str = "ssh_passphrase";

/// Store the SSH passphrase in the OS credential store.
pub fn set_ssh_passphrase(passphrase: &str) -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, SSH_PASSPHRASE_KEY)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    entry
        .set_password(passphrase)
        .map_err(|e| format!("Failed to store passphrase: {}", e))
}

/// Retrieve the SSH passphrase from the OS credential store.
/// Returns Ok(None) if no passphrase is stored.
pub fn get_ssh_passphrase() -> Result<Option<String>, String> {
    let entry = Entry::new(SERVICE_NAME, SSH_PASSPHRASE_KEY)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    match entry.get_password() {
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to retrieve passphrase: {}", e)),
    }
}

/// Delete the SSH passphrase from the OS credential store.
pub fn delete_ssh_passphrase() -> Result<(), String> {
    let entry = Entry::new(SERVICE_NAME, SSH_PASSPHRASE_KEY)
        .map_err(|e| format!("Failed to create keyring entry: {}", e))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // Already deleted, that's fine
        Err(e) => Err(format!("Failed to delete passphrase: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: These tests require a running OS keychain/credential store.
    // They are integration tests and may prompt for permissions on macOS.
    // Run manually with: cargo test --lib credential_store -- --ignored

    #[test]
    #[ignore] // Requires OS keychain access
    fn test_set_get_delete_passphrase_roundtrip() {
        let test_passphrase = "test-passphrase-12345";

        // Store
        set_ssh_passphrase(test_passphrase).unwrap();

        // Retrieve
        let retrieved = get_ssh_passphrase().unwrap();
        assert_eq!(retrieved, Some(test_passphrase.to_string()));

        // Delete
        delete_ssh_passphrase().unwrap();

        // Verify deleted
        let after_delete = get_ssh_passphrase().unwrap();
        assert_eq!(after_delete, None);
    }

    #[test]
    #[ignore] // Requires OS keychain access
    fn test_get_nonexistent_passphrase_returns_none() {
        // Ensure it's deleted first
        let _ = delete_ssh_passphrase();
        let result = get_ssh_passphrase().unwrap();
        assert_eq!(result, None);
    }
}
```

### Step 4: Register module in lib.rs

Add `mod credential_store;` at the top of `src-tauri/src/lib.rs` (after line 2):

```rust
mod git_operations;
mod models;
mod credential_store;
```

### Step 5: Run cargo check to verify module compiles

Run: `cargo check`
Working dir: `src-tauri`
Expected: Compiles successfully.

### Step 6: Remove `ssh_passphrase` from Settings struct

In `src-tauri/src/models.rs`, remove line 64 (`pub ssh_passphrase: Option<String>,`) from the Settings struct:

```rust
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Settings {
    pub user_name: String,
    pub user_email: String,
    pub ssh_key_path: Option<String>,
    pub theme: String,
    pub recent_repositories: Vec<String>,
    pub last_opened_repository: Option<String>,
}
```

### Step 7: Update `load_settings_from_disk` defaults

In `src-tauri/src/lib.rs`, update the default Settings in `load_settings_from_disk` (around line 131). Remove the `ssh_passphrase: None,` line:

```rust
Settings {
    user_name: String::new(),
    user_email: String::new(),
    ssh_key_path: None,
    theme: "dark".to_string(),
    recent_repositories: Vec::new(),
    last_opened_repository: None,
}
```

### Step 8: Update all sites that read `state.settings.ssh_passphrase`

There are 4 sites in `lib.rs` that read `state.settings.ssh_passphrase.clone()`. Replace each with a call to `credential_store::get_ssh_passphrase()`:

**Site 1 — `clone_repository` command (lib.rs:186-189):**

Replace:
```rust
let (ssh_key, ssh_pass) = {
    let state_lock = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    (state_lock.settings.ssh_key_path.clone(), state_lock.settings.ssh_passphrase.clone())
};
```
With:
```rust
let ssh_key = {
    let state_lock = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    state_lock.settings.ssh_key_path.clone()
};
let ssh_pass = credential_store::get_ssh_passphrase().ok().flatten();
```

**Site 2 — `push_changes` command (lib.rs:311-316):**

Replace:
```rust
let (path, ssh_key, ssh_pass) = {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    let path = repo.workdir().ok_or(AppError::Git("No workdir".to_string()))?.to_path_buf();
    (path, state.settings.ssh_key_path.clone(), state.settings.ssh_passphrase.clone())
};
```
With:
```rust
let (path, ssh_key) = {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    let path = repo.workdir().ok_or(AppError::Git("No workdir".to_string()))?.to_path_buf();
    (path, state.settings.ssh_key_path.clone())
};
let ssh_pass = credential_store::get_ssh_passphrase().ok().flatten();
```

**Site 3 — `pull_changes` command (lib.rs:331-337):** Same pattern as Site 2.

**Site 4 — `fetch_changes` command (lib.rs:352-358):** Same pattern as Site 2.

### Step 9: Add new Tauri commands for passphrase management

Add these commands in `lib.rs` (near the existing `get_settings`/`save_settings` commands, around line 452):

```rust
#[tauri::command]
fn set_ssh_passphrase(passphrase: String) -> AppResult<()> {
    credential_store::set_ssh_passphrase(&passphrase).map_err(AppError::Config)
}

#[tauri::command]
fn get_ssh_passphrase_cmd() -> AppResult<Option<String>> {
    credential_store::get_ssh_passphrase().map_err(AppError::Config)
}

#[tauri::command]
fn delete_ssh_passphrase() -> AppResult<()> {
    credential_store::delete_ssh_passphrase().map_err(AppError::Config)
}
```

Note: The Tauri command for get is named `get_ssh_passphrase_cmd` to avoid name collision with the `credential_store::get_ssh_passphrase` function. Alternatively, use full path qualification.

### Step 10: Register new commands in `invoke_handler`

Add the three new commands to the `tauri::generate_handler!` macro in `run()` (around line 578):

```rust
.invoke_handler(tauri::generate_handler![
    // ... existing commands ...
    set_ssh_passphrase,
    get_ssh_passphrase_cmd,
    delete_ssh_passphrase,
])
```

### Step 11: Run cargo check to verify Rust compiles

Run: `cargo check`
Working dir: `src-tauri`
Expected: Compiles successfully. Fix any compilation errors related to the destructuring changes.

### Step 12: Update TypeScript Settings interface

In `src/services/git.ts`, remove `ssh_passphrase` from the Settings interface (line 85):

```typescript
export interface Settings {
  user_name: string;
  user_email: string;
  ssh_key_path: string | null;
  theme: string;
  recent_repositories: string[];
  last_opened_repository: string | null;
}
```

### Step 13: Add TypeScript wrappers for passphrase commands

Add these methods to the `gitService` object in `src/services/git.ts`:

```typescript
/**
 * Store SSH passphrase in OS secure credential store
 */
async setSshPassphrase(passphrase: string): Promise<void> {
  return await invoke("set_ssh_passphrase", { passphrase });
},

/**
 * Retrieve SSH passphrase from OS secure credential store
 */
async getSshPassphrase(): Promise<string | null> {
  return await invoke("get_ssh_passphrase_cmd");
},

/**
 * Delete SSH passphrase from OS secure credential store
 */
async deleteSshPassphrase(): Promise<void> {
  return await invoke("delete_ssh_passphrase");
},
```

### Step 14: Update frontend Settings UI (App.vue)

Search `App.vue` for any references to `ssh_passphrase` and remove them. Since the exploration found NO UI input for ssh_passphrase, the main concern is that the `settings` reactive object might still have the field from `getSettings()`. Since we removed it from the Rust struct, it simply won't be in the returned JSON anymore — no frontend change needed for display.

However, if there is any code that sends `settings.ssh_passphrase` back via `saveSettings()`, that will need to be cleaned up. Search for `ssh_passphrase` in `App.vue` and remove any references.

### Step 15: Handle migration — clean up existing settings.json

`serde_json::from_str` with `Deserialize` already ignores unknown fields by default, so old `settings.json` files with `ssh_passphrase` will have that field silently dropped. No migration code is strictly needed.

Optionally, add a one-time migration in the setup to move the plaintext passphrase to keyring. In `lib.rs` `run()` setup, after loading settings:

```rust
.setup(|app| {
    let app_handle = app.handle();
    let settings = load_settings_from_disk(app_handle);

    // One-time migration: move plaintext passphrase to keyring
    migrate_passphrase_to_keyring(app_handle);

    // ... rest of setup
})
```

Create the migration helper in `lib.rs`:

```rust
fn migrate_passphrase_to_keyring(app_handle: &tauri::AppHandle) {
    // Read raw JSON to check for old ssh_passphrase field
    if let Ok(path) = get_settings_path(app_handle) {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(raw) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(passphrase) = raw.get("ssh_passphrase").and_then(|v| v.as_str()) {
                    if !passphrase.is_empty() {
                        // Store in keyring
                        let _ = credential_store::set_ssh_passphrase(passphrase);
                    }
                    // Re-write settings.json without ssh_passphrase
                    // (next save_settings_to_disk call will do this automatically
                    // since the field is no longer in the struct)
                }
            }
        }
    }
}
```

### Step 16: Run full build to verify everything compiles

Run: `cargo build`
Working dir: `src-tauri`
Expected: Compiles successfully.

### Step 17: Run all Rust tests

Run: `cargo test --lib`
Working dir: `src-tauri`
Expected: ALL existing tests pass. The keyring integration tests are `#[ignore]`d by default.

### Step 18: Commit

```bash
git add src-tauri/Cargo.toml src-tauri/src/credential_store.rs \
  src-tauri/src/models.rs src-tauri/src/lib.rs \
  src/services/git.ts
git commit -m "security: replace plaintext ssh_passphrase with OS keychain storage

Remove ssh_passphrase from the Settings struct to prevent serialization
to settings.json. Add credential_store module using the keyring crate
for cross-platform OS-level secure storage (macOS Keychain, Windows
Credential Manager, Linux Secret Service). Add Tauri commands for
passphrase get/set/delete. Include one-time migration from existing
plaintext settings.json to keyring on startup."
```

---

## Verification Checklist

After all 3 tasks are complete, verify:

- [ ] `cargo test --lib` in `src-tauri/` — all tests pass
- [ ] `cargo clippy` in `src-tauri/` — no warnings
- [ ] `cargo build` in `src-tauri/` — builds successfully
- [ ] Manual test: attempt to discard `../../../etc/passwd` — should return error
- [ ] Manual test: resolve conflict with "Use Ours" — file should contain ours version
- [ ] Manual test: resolve conflict with "Use Theirs" — file should contain theirs version
- [ ] Manual test: set SSH passphrase via settings — check it's NOT in `settings.json`
- [ ] Manual test: verify SSH passphrase appears in OS keychain (macOS: Keychain Access)
- [ ] `settings.json` no longer contains `ssh_passphrase` field after save
- [ ] Old `settings.json` with plaintext passphrase is migrated on first startup

## Risk Notes

1. **keyring crate feature names**: The exact feature flag names for keyring 3.x may vary. Verify at https://docs.rs/keyring/3 during execution. Fallback: use default features (`keyring = "3"`).
2. **Linux CI**: Secret Service requires a running DBus session. CI environments may not have this. Consider adding `#[cfg(not(target_os = "linux"))]` conditional compilation or mocking for CI.
3. **Serde backward compatibility**: Removing a field from `Settings` is backward-compatible for deserialization (serde ignores unknown fields). But the migration helper should run once to salvage existing passphrases.
4. **git2 conflict entries**: If a file was deleted on one side (delete/modify conflict), `conflict.our` or `conflict.their` will be `None`. The implementation handles this with `ok_or_else`.
5. **glib dependency**: The project has `glib = "0.20"` in Cargo.toml which is a Linux-specific dependency. This may cause issues on macOS/Windows builds. Not related to this security fix but worth noting.
