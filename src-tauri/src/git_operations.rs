use git2::{BranchType, DiffOptions, Repository, Signature, StashFlags, StatusOptions};
use std::fs;
use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use crate::models::{
    BranchInfo, CommitInfo, ConflictInfo, DiffInfo, FileStatus, RepositoryInfo, StageResult,
    StashInfo,
};

pub fn open_repository(path: &str) -> Result<Repository, String> {
    Repository::open(path).map_err(|e| format!("Failed to open repository: {}", e))
}

/// Executes a git command safely.
/// Prevents shell injection by using Command::args directly.
/// Sanitizes critical inputs like URLs and branch names in caller functions.
fn run_git_command(
    args: Vec<&str>,
    cwd: Option<&str>,
    envs: Vec<(&str, String)>,
) -> Result<String, String> {
    let mut command = Command::new("git");

    // Explicitly set NO_PAGER to avoid interactive sessions
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.env("GIT_PAGER", "cat");

    command.args(&args);

    if let Some(path) = cwd {
        command.current_dir(path);
    }

    for (key, val) in envs {
        command.env(key, val);
    }

    let output = command
        .output()
        .map_err(|e| format!("Failed to execute git command: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

        Err(if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("Git command failed with status: {}", output.status)
        })
    }
}

fn is_safe_git_arg(arg: &str) -> bool {
    // Prevent common shell/command injection patterns and flag injection
    !arg.is_empty()
        && !arg.starts_with('-')
        && !arg.contains(' ')
        && !arg.contains(';')
        && !arg.contains('&')
        && !arg.contains('|')
        && !arg.contains('`')
        && !arg.contains('$')
        && !arg.contains('\\')
}

fn shell_escape_single_quotes(value: &str) -> String {
    value.replace('\'', "'\\''")
}

fn build_git_ssh_command(ssh_key_path: Option<&str>) -> Result<Option<String>, String> {
    let Some(key) = ssh_key_path else {
        return Ok(None);
    };

    if key.trim().is_empty() {
        return Ok(None);
    }

    let expanded_path = if key.starts_with("~/") {
        let home =
            std::env::var("HOME").map_err(|_| "Could not find HOME directory".to_string())?;
        key.replacen("~", &home, 1)
    } else {
        key.to_string()
    };

    if !Path::new(&expanded_path).exists() {
        return Err(format!("SSH key file does not exist: {}", expanded_path));
    }

    let escaped_path = shell_escape_single_quotes(&expanded_path);
    Ok(Some(format!(
        "ssh -i '{}' -o IdentitiesOnly=yes",
        escaped_path
    )))
}

pub fn clone_repository(
    url: &str,
    path: &str,
    ssh_key_path: Option<&str>,
    _ssh_passphrase: Option<&str>,
) -> Result<Repository, String> {
    if url.contains(' ') || url.contains(';') || url.starts_with('-') {
        return Err("Invalid clone URL".to_string());
    }

    let mut envs = Vec::new();
    if let Some(command) = build_git_ssh_command(ssh_key_path)? {
        envs.push(("GIT_SSH_COMMAND", command));
    }
    run_git_command(vec!["clone", "--", url, path], None, envs)?;
    open_repository(path)
}

pub fn get_repository_info(repo: &Repository) -> Result<RepositoryInfo, String> {
    let mut ahead = 0;
    let mut behind = 0;
    let mut current_branch = "unknown".to_string();

    match repo.head() {
        Ok(head) => {
            current_branch = if head.is_branch() {
                head.shorthand().unwrap_or("unknown").to_string()
            } else {
                "detached HEAD".to_string()
            };

            if head.is_branch() {
                if let (Some(local_name), Some(local_oid)) = (head.name(), head.target()) {
                    if let Ok(upstream) = repo.branch_upstream_name(local_name) {
                        if let Some(upstream_name) = upstream.as_str() {
                            if let Ok(upstream_ref) = repo.find_reference(upstream_name) {
                                if let Some(upstream_oid) = upstream_ref.target() {
                                    if let Ok((a, b)) =
                                        repo.graph_ahead_behind(local_oid, upstream_oid)
                                    {
                                        ahead = a;
                                        behind = b;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Err(_) => {
            // Probably an unborn branch or empty repo
            if let Ok(head_ref) = repo.find_reference("HEAD") {
                if let Some(name) = head_ref.symbolic_target() {
                    current_branch = name.strip_prefix("refs/heads/").unwrap_or(name).to_string();
                }
            }
        }
    }

    let statuses = repo
        .statuses(None)
        .map_err(|e| format!("Failed to get statuses: {}", e))?;

    let is_dirty = !statuses.is_empty();

    let mut path = repo
        .workdir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| repo.path().to_string_lossy().to_string());

    // 移除末尾斜線，確保路徑格式一致
    while path.ends_with('/') || path.ends_with('\\') {
        path.pop();
    }

    Ok(RepositoryInfo {
        path,
        current_branch,
        is_dirty,
        ahead,
        behind,
    })
}

pub fn get_status(repo: &Repository) -> Result<Vec<FileStatus>, String> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true);
    opts.recurse_untracked_dirs(true);

    let statuses = repo
        .statuses(Some(&mut opts))
        .map_err(|e| format!("Failed to get status: {}", e))?;

    let mut file_statuses = Vec::new();

    for entry in statuses.iter() {
        let status = entry.status();
        let path = entry.path().unwrap_or("unknown").to_string();

        let status_str =
            if status.is_index_new() || status.is_index_modified() || status.is_index_deleted() {
                if status.is_index_new() {
                    "added"
                } else if status.is_index_modified() {
                    "modified"
                } else {
                    "deleted"
                }
            } else if status.is_wt_new() {
                "untracked"
            } else if status.is_wt_modified() {
                "modified"
            } else if status.is_wt_deleted() {
                "deleted"
            } else {
                "unknown"
            };

        let staged =
            status.is_index_new() || status.is_index_modified() || status.is_index_deleted();

        file_statuses.push(FileStatus {
            path,
            status: status_str.to_string(),
            staged,
        });
    }

    Ok(file_statuses)
}

fn validate_repo_path(repo: &Repository, path: &str) -> Result<PathBuf, String> {
    if path.is_empty() {
        return Err("Path cannot be empty".to_string());
    }

    if path.starts_with('/') || path.starts_with('\\') || Path::new(path).is_absolute() {
        return Err("Absolute paths are not allowed".to_string());
    }

    let relative_path = Path::new(path);
    if relative_path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err("Path traversal is not allowed".to_string());
    }

    let workdir = repo.workdir().ok_or("No working directory found")?;
    let canonical_workdir = workdir
        .canonicalize()
        .map_err(|e| format!("Failed to resolve repository path: {}", e))?;
    let full_path = workdir.join(relative_path);

    let mut current = workdir.to_path_buf();
    for component in relative_path.components() {
        match component {
            Component::CurDir => continue,
            Component::Normal(part) => {
                current.push(part);

                match fs::symlink_metadata(&current) {
                    Ok(_) => {
                        let canonical_path = current
                            .canonicalize()
                            .map_err(|e| format!("Failed to resolve path '{}': {}", path, e))?;

                        if !canonical_path.starts_with(&canonical_workdir) {
                            return Err(format!("Path '{}' resolves outside the repository", path));
                        }
                    }
                    Err(err) if err.kind() == ErrorKind::NotFound => {
                        let parent = current.parent().unwrap_or(workdir);
                        let canonical_parent = parent.canonicalize().map_err(|e| {
                            format!("Failed to resolve parent for '{}': {}", path, e)
                        })?;

                        if !canonical_parent.starts_with(&canonical_workdir) {
                            return Err(format!("Path '{}' resolves outside the repository", path));
                        }

                        break;
                    }
                    Err(err) => {
                        return Err(format!("Failed to inspect path '{}': {}", path, err));
                    }
                }
            }
            Component::ParentDir => return Err("Path traversal is not allowed".to_string()),
            Component::RootDir | Component::Prefix(_) => {
                return Err("Absolute paths are not allowed".to_string())
            }
        }
    }

    Ok(full_path)
}

fn validate_workdir_entries(repo: &Repository) -> Result<(), String> {
    fn visit(repo: &Repository, root: &Path, dir: &Path) -> Result<(), String> {
        for entry in fs::read_dir(dir).map_err(|e| format!("Failed to scan repository: {}", e))? {
            let entry = entry.map_err(|e| format!("Failed to read repository entry: {}", e))?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)
                .map_err(|_| "Failed to normalize repository entry".to_string())?;

            if relative == Path::new(".git") {
                continue;
            }

            let relative_str = relative.to_string_lossy().replace('\\', "/");
            validate_repo_path(repo, &relative_str)?;

            let metadata = fs::symlink_metadata(&path)
                .map_err(|e| format!("Failed to inspect repository entry: {}", e))?;
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                visit(repo, root, &path)?;
            }
        }

        Ok(())
    }

    let workdir = repo.workdir().ok_or("No working directory found")?;
    visit(repo, workdir, workdir)
}

pub fn stage_files(repo: &Repository, paths: Vec<String>) -> Result<StageResult, String> {
    let mut index = repo
        .index()
        .map_err(|e| format!("Failed to get index: {}", e))?;

    let workdir = repo.workdir().ok_or("No working directory found")?;
    let mut staged = Vec::new();
    let mut warnings = Vec::new();

    for path in paths {
        let full_path = match validate_repo_path(repo, &path) {
            Ok(full_path) => full_path,
            Err(err) => {
                warnings.push(format!("Skipped '{}': {}", path, err));
                continue;
            }
        };
        let relative_path = full_path
            .strip_prefix(workdir)
            .map_err(|_| format!("Validated path '{}' is outside the repository", path))?;

        if full_path.exists() {
            match index.add_path(relative_path) {
                Ok(_) => staged.push(path),
                Err(e) => warnings.push(format!("Failed to stage '{}': {}", path, e)),
            }
        } else {
            // File was deleted externally — clean up index entry and record warning
            let _ = index.remove_path(relative_path);
            warnings.push(format!(
                "Skipped '{}': file not found (removed from index)",
                path
            ));
        }
    }

    index
        .write()
        .map_err(|e| format!("Failed to write index: {}", e))?;

    Ok(StageResult { staged, warnings })
}

pub fn unstage_files(repo: &Repository, paths: Vec<String>) -> Result<(), String> {
    let workdir = repo.workdir().ok_or("No working directory found")?;
    let validated_paths = paths
        .iter()
        .map(|path| {
            let full_path = validate_repo_path(repo, path)?;
            let relative_path = full_path
                .strip_prefix(workdir)
                .map_err(|_| format!("Validated path '{}' is outside the repository", path))?;
            Ok(relative_path.to_string_lossy().into_owned())
        })
        .collect::<Result<Vec<_>, String>>()?;

    let head = repo.head().ok();
    let commit = head.and_then(|h| h.peel_to_commit().ok());

    if let Some(c) = commit {
        // Try bulk reset first; if it fails, fall back to per-file reset
        if repo
            .reset_default(
                Some(c.as_object()),
                validated_paths.iter().map(|s| s.as_str()),
            )
            .is_err()
        {
            for path in &validated_paths {
                let _ = repo.reset_default(Some(c.as_object()), std::iter::once(path.as_str()));
            }
        }
    } else {
        // No commits yet, just remove from index
        let mut index = repo
            .index()
            .map_err(|e| format!("Failed to get index: {}", e))?;
        for path in validated_paths {
            index.remove_path(Path::new(&path)).ok();
        }
        index
            .write()
            .map_err(|e| format!("Failed to write index: {}", e))?;
    }

    Ok(())
}

pub fn create_safety_ref(repo: &Repository, action_name: &str) -> Result<(), String> {
    let head = match repo.head() {
        Ok(h) => h,
        Err(_) => return Ok(()), // No HEAD yet, nothing to snapshot
    };
    let commit = head.peel_to_commit().map_err(|e| e.to_string())?;
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Use a specific namespace for safety refs
    let ref_name = format!("refs/safety/{}/{}", action_name, timestamp);
    repo.reference(
        &ref_name,
        commit.id(),
        true,
        &format!("safety snapshot before {}", action_name),
    )
    .map_err(|e| format!("Failed to create safety ref: {}", e))?;
    Ok(())
}

pub fn amend_last_commit(repo: &Repository, message: &str) -> Result<String, String> {
    create_safety_ref(repo, "amend")?;
    let mut index = repo
        .index()
        .map_err(|e| format!("Failed to get index: {}", e))?;

    let tree_id = index
        .write_tree()
        .map_err(|e| format!("Failed to write tree: {}", e))?;

    let tree = repo
        .find_tree(tree_id)
        .map_err(|e| format!("Failed to find tree: {}", e))?;

    let signature = repo
        .signature()
        .or_else(|_| Signature::now("User", "user@example.com"))
        .map_err(|e| format!("Failed to create signature: {}", e))?;

    let head = repo
        .head()
        .map_err(|e| format!("Failed to get HEAD: {}", e))?;
    let last_commit = head
        .peel_to_commit()
        .map_err(|e| format!("Failed to peel HEAD to commit: {}", e))?;

    let commit_id = last_commit
        .amend(
            Some("HEAD"),
            Some(&signature),
            Some(&signature),
            None,
            Some(message),
            Some(&tree),
        )
        .map_err(|e| format!("Failed to amend commit: {}", e))?;

    Ok(commit_id.to_string())
}

pub fn cherry_pick(repo: &Repository, sha: &str) -> Result<(), String> {
    create_safety_ref(repo, "cherry-pick")?;
    let commit = repo
        .find_commit(git2::Oid::from_str(sha).map_err(|e| e.to_string())?)
        .map_err(|e| format!("Commit not found: {}", e))?;

    let mut opts = git2::CherrypickOptions::new();
    repo.cherrypick(&commit, Some(&mut opts))
        .map_err(|e| format!("Cherry-pick failed: {}", e))?;

    let mut index = repo.index().map_err(|e| e.to_string())?;
    if index.has_conflicts() {
        return Err("Cherry-pick resulted in conflicts. Please resolve them.".to_string());
    }

    let tree_id = index.write_tree().map_err(|e| e.to_string())?;
    let tree = repo.find_tree(tree_id).map_err(|e| e.to_string())?;
    let signature = repo.signature().map_err(|e| e.to_string())?;

    let head = repo
        .head()
        .map_err(|e| format!("Failed to get HEAD: {}", e))?;
    let parent = head
        .peel_to_commit()
        .map_err(|e| format!("Failed to peel HEAD: {}", e))?;

    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        commit.message().unwrap_or("Cherry-picked commit"),
        &tree,
        &[&parent],
    )
    .map_err(|e| e.to_string())?;

    repo.cleanup_state().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn revert_commit(repo: &Repository, sha: &str) -> Result<(), String> {
    create_safety_ref(repo, "revert")?;
    let commit = repo
        .find_commit(git2::Oid::from_str(sha).map_err(|e| e.to_string())?)
        .map_err(|e| format!("Commit not found: {}", e))?;

    let mut opts = git2::RevertOptions::new();
    repo.revert(&commit, Some(&mut opts))
        .map_err(|e| format!("Revert failed: {}", e))?;

    let mut index = repo.index().map_err(|e| e.to_string())?;
    if index.has_conflicts() {
        return Err("Revert resulted in conflicts. Please resolve them.".to_string());
    }

    let tree_id = index.write_tree().map_err(|e| e.to_string())?;
    let tree = repo.find_tree(tree_id).map_err(|e| e.to_string())?;
    let signature = repo.signature().map_err(|e| e.to_string())?;

    let head = repo
        .head()
        .map_err(|e| format!("Failed to get HEAD: {}", e))?;
    let parent = head
        .peel_to_commit()
        .map_err(|e| format!("Failed to peel HEAD: {}", e))?;

    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        &format!("Revert \"{}\"", commit.message().unwrap_or("")),
        &tree,
        &[&parent],
    )
    .map_err(|e| e.to_string())?;

    repo.cleanup_state().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn discard_changes(repo: &Repository, path: &str) -> Result<(), String> {
    let workdir = repo.workdir().ok_or("No workdir")?;
    let full_path = validate_repo_path(repo, path)?;
    let relative_path = full_path
        .strip_prefix(workdir)
        .map_err(|_| format!("Validated path '{}' is outside the repository", path))?;

    let mut checkout_opts = git2::build::CheckoutBuilder::new();
    checkout_opts.force().path(relative_path);

    // Attempt checkout from HEAD
    if repo.checkout_head(Some(&mut checkout_opts)).is_err() {
        // If checkout head fails (e.g. untracked file), try to remove it
        if full_path.exists() {
            let metadata = fs::symlink_metadata(&full_path)
                .map_err(|e| format!("Failed to inspect path '{}': {}", path, e))?;

            if metadata.file_type().is_symlink() || metadata.is_file() {
                fs::remove_file(full_path).map_err(|e| format!("Failed to delete file: {}", e))?;
            } else if metadata.is_dir() {
                fs::remove_dir_all(full_path)
                    .map_err(|e| format!("Failed to delete dir: {}", e))?;
            }
        }
    }

    Ok(())
}

pub fn discard_all_changes(repo: &Repository) -> Result<(), String> {
    validate_workdir_entries(repo)?;

    let mut status_opts = StatusOptions::new();
    status_opts.include_untracked(true);
    status_opts.recurse_untracked_dirs(true);

    let statuses = repo
        .statuses(Some(&mut status_opts))
        .map_err(|e| format!("Failed to get status: {}", e))?;

    for entry in statuses.iter() {
        if let Some(path) = entry.path() {
            validate_repo_path(repo, path)?;
        }
    }

    let _ = create_safety_ref(repo, "discard-all");
    let mut checkout_opts = git2::build::CheckoutBuilder::new();
    checkout_opts.force();
    repo.checkout_head(Some(&mut checkout_opts))
        .map_err(|e| format!("Failed to discard all changes: {}", e))
}

pub fn create_branch(repo: &Repository, name: &str, start_sha: Option<&str>) -> Result<(), String> {
    if !is_safe_git_arg(name) {
        return Err("Invalid branch name".to_string());
    }

    let commit = match start_sha {
        Some(sha) => repo
            .find_commit(git2::Oid::from_str(sha).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Commit not found: {}", e))?,
        None => {
            let head = repo
                .head()
                .map_err(|e| format!("Failed to get HEAD: {}", e))?;
            head.peel_to_commit()
                .map_err(|e| format!("Failed to peel HEAD to commit: {}", e))?
        }
    };

    repo.branch(name, &commit, false)
        .map_err(|e| format!("Failed to create branch: {}", e))?;

    checkout_branch(repo, name)
}

pub fn get_commit_diff(repo: &Repository, sha: &str) -> Result<Vec<DiffInfo>, String> {
    let commit = repo
        .find_commit(git2::Oid::from_str(sha).map_err(|e| e.to_string())?)
        .map_err(|e| format!("Commit not found: {}", e))?;

    let tree = commit
        .tree()
        .map_err(|e| format!("Failed to get tree: {}", e))?;
    let parent_tree = if commit.parent_count() > 0 {
        Some(
            commit
                .parent(0)
                .map_err(|e| e.to_string())?
                .tree()
                .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };

    let mut diff_opts = DiffOptions::new();
    let diff = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), Some(&mut diff_opts))
        .map_err(|e| format!("Failed to generate diff: {}", e))?;

    let mut diff_infos = Vec::new();
    diff.print(git2::DiffFormat::Patch, |delta, _hunk, line| {
        let path = delta
            .new_file()
            .path()
            .and_then(|p| p.to_str())
            .unwrap_or("unknown")
            .to_string();

        let line_content = String::from_utf8_lossy(line.content()).to_string();
        let prefix = match line.origin() {
            '+' => "+",
            '-' => "-",
            ' ' => " ",
            _ => "",
        };

        if let Some(info) = diff_infos
            .iter_mut()
            .find(|i: &&mut DiffInfo| i.path == path)
        {
            info.diff_text
                .push_str(&format!("{}{}", prefix, line_content));
            match line.origin() {
                '+' => info.additions += 1,
                '-' => info.deletions += 1,
                _ => {}
            }
        } else {
            diff_infos.push(DiffInfo {
                path,
                diff_text: format!("{}{}", prefix, line_content),
                additions: if line.origin() == '+' { 1 } else { 0 },
                deletions: if line.origin() == '-' { 1 } else { 0 },
            });
        }
        true
    })
    .map_err(|e| format!("Failed to parse diff: {}", e))?;

    Ok(diff_infos)
}

pub fn create_commit(repo: &Repository, message: &str) -> Result<String, String> {
    let mut index = repo
        .index()
        .map_err(|e| format!("Failed to get index: {}", e))?;

    let tree_id = index
        .write_tree()
        .map_err(|e| format!("Failed to write tree: {}", e))?;

    let tree = repo
        .find_tree(tree_id)
        .map_err(|e| format!("Failed to find tree: {}", e))?;

    let signature = repo
        .signature()
        .or_else(|_| Signature::now("User", "user@example.com"))
        .map_err(|e| format!("Failed to create signature: {}", e))?;

    let head = repo.head().ok();
    let parent_commit = head.as_ref().and_then(|h| h.peel_to_commit().ok());

    let parents = if let Some(ref parent) = parent_commit {
        vec![parent]
    } else {
        vec![]
    };

    let parent_refs: Vec<&git2::Commit> = parents.iter().map(|c| *c).collect();
    let commit_id = repo
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )
        .map_err(|e| format!("Failed to create commit: {}", e))?;

    Ok(commit_id.to_string())
}

pub fn get_branches(repo: &Repository) -> Result<Vec<BranchInfo>, String> {
    let branches = repo
        .branches(Some(BranchType::Local))
        .map_err(|e| format!("Failed to get branches: {}", e))?;

    let head = repo.head().ok();
    let current_branch_name = head
        .as_ref()
        .and_then(|h| h.shorthand())
        .map(|s| s.to_string());

    let mut branch_list = Vec::new();

    for branch_result in branches {
        let (branch, _) = branch_result.map_err(|e| format!("Failed to read branch: {}", e))?;
        let name = branch
            .name()
            .map_err(|e| format!("Failed to get branch name: {}", e))?
            .unwrap_or("unknown")
            .to_string();

        let is_current = current_branch_name.as_ref() == Some(&name);

        branch_list.push(BranchInfo {
            name,
            is_current,
            is_remote: false,
        });
    }

    Ok(branch_list)
}

pub fn checkout_branch(repo: &Repository, name: &str) -> Result<(), String> {
    if !is_safe_git_arg(name) {
        return Err("Invalid branch name".to_string());
    }
    let obj = repo
        .revparse_single(&format!("refs/heads/{}", name))
        .map_err(|e| format!("Failed to find branch: {}", e))?;

    repo.checkout_tree(&obj, None)
        .map_err(|e| format!("Failed to checkout tree: {}", e))?;

    repo.set_head(&format!("refs/heads/{}", name))
        .map_err(|e| format!("Failed to set HEAD: {}", e))?;

    Ok(())
}

pub fn get_commit_history(repo: &Repository, limit: usize) -> Result<Vec<CommitInfo>, String> {
    let head = repo.head().ok();

    // Get upstream OID to check for pushed status
    let upstream_oid = head.as_ref().and_then(|h| {
        if h.is_branch() {
            h.name().and_then(|name| {
                repo.branch_upstream_name(name).ok().and_then(|upstream| {
                    upstream
                        .as_str()
                        .and_then(|u_name| repo.find_reference(u_name).ok())
                        .and_then(|r| r.target())
                })
            })
        } else {
            None
        }
    });

    let mut revwalk = repo
        .revwalk()
        .map_err(|e| format!("Failed to create revwalk: {}", e))?;

    revwalk
        .push_head()
        .map_err(|e| format!("Failed to push HEAD: {}", e))?;

    let mut commits = Vec::new();

    for (i, oid) in revwalk.enumerate() {
        if i >= limit {
            break;
        }

        let oid = oid.map_err(|e| format!("Failed to get OID: {}", e))?;
        let commit = repo
            .find_commit(oid)
            .map_err(|e| format!("Failed to find commit: {}", e))?;

        // Logic: if upstream can reach this commit, it is pushed.
        let is_pushed = if let Some(u_oid) = upstream_oid {
            repo.graph_descendant_of(u_oid, oid).unwrap_or(false) || u_oid == oid
        } else {
            false
        };

        commits.push(CommitInfo {
            sha: commit.id().to_string(),
            message: commit.message().unwrap_or("").to_string(),
            author: commit.author().name().unwrap_or("Unknown").to_string(),
            email: commit.author().email().unwrap_or("").to_string(),
            timestamp: commit.time().seconds(),
            is_pushed,
            parents: commit.parent_ids().map(|id| id.to_string()).collect(),
        });
    }

    Ok(commits)
}

pub fn get_diff(repo: &Repository, path: Option<&str>) -> Result<Vec<DiffInfo>, String> {
    let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());

    let mut opts = DiffOptions::new();
    if let Some(p) = path {
        opts.pathspec(p);
    }

    let diff = if let Some(tree) = head_tree {
        repo.diff_tree_to_workdir_with_index(Some(&tree), Some(&mut opts))
            .map_err(|e| format!("Failed to get diff (tree to workdir): {}", e))?
    } else {
        repo.diff_index_to_workdir(None, Some(&mut opts))
            .map_err(|e| format!("Failed to get diff (index to workdir): {}", e))?
    };

    let mut diff_infos = Vec::new();

    diff.print(git2::DiffFormat::Patch, |delta, _hunk, line| {
        let file_path = delta
            .new_file()
            .path()
            .and_then(|p| p.to_str())
            .unwrap_or("unknown")
            .to_string();

        let line_content = String::from_utf8_lossy(line.content()).to_string();
        let prefix = match line.origin() {
            '+' => "+",
            '-' => "-",
            ' ' => " ",
            _ => "",
        };

        if let Some(info) = diff_infos
            .iter_mut()
            .find(|i: &&mut DiffInfo| i.path == file_path)
        {
            info.diff_text
                .push_str(&format!("{}{}", prefix, line_content));
            match line.origin() {
                '+' => info.additions += 1,
                '-' => info.deletions += 1,
                _ => {}
            }
        } else {
            diff_infos.push(DiffInfo {
                path: file_path,
                diff_text: format!("{}{}", prefix, line_content),
                additions: if line.origin() == '+' { 1 } else { 0 },
                deletions: if line.origin() == '-' { 1 } else { 0 },
            });
        }
        true
    })
    .map_err(|e| format!("Failed to parse diff: {}", e))?;

    Ok(diff_infos)
}

pub fn push_changes(
    repo: &Repository,
    ssh_key_path: Option<&str>,
    _ssh_passphrase: Option<&str>,
) -> Result<(), String> {
    let path = repo
        .workdir()
        .ok_or("No working directory found")?
        .to_str()
        .ok_or("Invalid path")?;
    let mut envs = Vec::new();
    if let Some(command) = build_git_ssh_command(ssh_key_path)? {
        envs.push(("GIT_SSH_COMMAND", command));
    }

    run_git_command(vec!["push", "origin", "HEAD"], Some(path), envs)?;
    Ok(())
}

pub fn pull_changes(
    repo: &Repository,
    ssh_key_path: Option<&str>,
    _ssh_passphrase: Option<&str>,
) -> Result<(), String> {
    let path = repo
        .workdir()
        .ok_or("No working directory found")?
        .to_str()
        .ok_or("Invalid path")?;
    let mut envs = Vec::new();
    if let Some(command) = build_git_ssh_command(ssh_key_path)? {
        envs.push(("GIT_SSH_COMMAND", command));
    }

    let head = repo
        .head()
        .map_err(|e| format!("Failed to get HEAD: {}", e))?;
    let branch_name = if head.is_branch() {
        head.shorthand().unwrap_or("HEAD")
    } else {
        "HEAD"
    };

    run_git_command(vec!["pull", "origin", branch_name], Some(path), envs)?;
    Ok(())
}

pub fn stash_save(repo: &mut Repository, message: Option<&str>) -> Result<(), String> {
    let signature = repo
        .signature()
        .or_else(|_| Signature::now("User", "user@example.com"))
        .map_err(|e| format!("Failed to create signature: {}", e))?;

    repo.stash_save(
        &signature,
        message.unwrap_or(""),
        Some(StashFlags::INCLUDE_UNTRACKED),
    )
    .map_err(|e| format!("Failed to stash: {}", e))?;

    Ok(())
}

pub fn stash_pop(repo: &mut Repository, index: usize) -> Result<(), String> {
    repo.stash_pop(index, None)
        .map_err(|e| format!("Failed to pop stash: {}", e))?;
    Ok(())
}

pub fn stash_list(repo: &mut Repository) -> Result<Vec<StashInfo>, String> {
    let mut stashes = Vec::new();
    repo.stash_foreach(|index, message, id| {
        stashes.push(StashInfo {
            index,
            message: message.to_string(),
            sha: id.to_string(),
        });
        true
    })
    .map_err(|e| format!("Failed to list stashes: {}", e))?;

    Ok(stashes)
}

pub fn get_conflicts(repo: &Repository) -> Result<Vec<ConflictInfo>, String> {
    let index = repo
        .index()
        .map_err(|e| format!("Failed to get index: {}", e))?;

    let mut conflicts = Vec::new();
    for conflict in index
        .conflicts()
        .map_err(|e| format!("Failed to get conflicts: {}", e))?
    {
        let conflict = conflict.map_err(|e| format!("Conflict error: {}", e))?;
        let path = conflict
            .ancestor
            .as_ref()
            .or(conflict.our.as_ref())
            .or(conflict.their.as_ref())
            .map(|e| String::from_utf8_lossy(&e.path).to_string())
            .unwrap_or_default();

        conflicts.push(ConflictInfo {
            path,
            our_status: if conflict.our.is_some() {
                "modified"
            } else {
                "deleted"
            }
            .to_string(),
            their_status: if conflict.their.is_some() {
                "modified"
            } else {
                "deleted"
            }
            .to_string(),
        });
    }

    Ok(conflicts)
}

pub fn resolve_conflict(repo: &Repository, path: &str, use_ours: bool) -> Result<(), String> {
    let mut index = repo
        .index()
        .map_err(|e| format!("Failed to get index: {}", e))?;

    let mut selected_blob_id = None;
    let mut conflict_found = false;

    for conflict in index
        .conflicts()
        .map_err(|e| format!("Failed to get conflicts: {}", e))?
    {
        let conflict = conflict.map_err(|e| format!("Conflict error: {}", e))?;
        let conflict_path = conflict
            .ancestor
            .as_ref()
            .or(conflict.our.as_ref())
            .or(conflict.their.as_ref())
            .map(|entry| String::from_utf8_lossy(&entry.path).to_string())
            .unwrap_or_default();

        if conflict_path != path {
            continue;
        }

        conflict_found = true;
        selected_blob_id = if use_ours {
            conflict.our.as_ref().map(|entry| entry.id)
        } else {
            conflict.their.as_ref().map(|entry| entry.id)
        };
        break;
    }

    if !conflict_found {
        return Err(format!("Failed to resolve '{}': conflict not found", path));
    }

    let workdir = repo.workdir().ok_or("No working directory found")?;
    let file_path = workdir.join(path);

    match selected_blob_id {
        Some(blob_id) => {
            let blob = repo
                .find_blob(blob_id)
                .map_err(|e| format!("Failed to read conflicted blob: {}", e))?;

            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Failed to prepare conflicted file path: {}", e))?;
            }

            fs::write(&file_path, blob.content())
                .map_err(|e| format!("Failed to write resolved file: {}", e))?;
        }
        None => {
            if file_path.exists() {
                fs::remove_file(&file_path)
                    .map_err(|e| format!("Failed to remove resolved file: {}", e))?;
            }
        }
    }

    index
        .remove_path(Path::new(path))
        .map_err(|e| format!("Failed to clear conflict: {}", e))?;

    if selected_blob_id.is_some() {
        index
            .add_path(Path::new(path))
            .map_err(|e| format!("Failed to resolve: {}", e))?;
    }

    index
        .write()
        .map_err(|e| format!("Failed to write index: {}", e))?;

    Ok(())
}

#[allow(dead_code)]
pub fn create_remote_callbacks() -> () {
    // Deprecated
}
pub fn fetch_changes(
    repo: &Repository,
    ssh_key_path: Option<&str>,
    _ssh_passphrase: Option<&str>,
) -> Result<(), String> {
    let path = repo
        .workdir()
        .ok_or("No working directory found")?
        .to_str()
        .ok_or("Invalid path")?;
    let mut envs = Vec::new();
    if let Some(command) = build_git_ssh_command(ssh_key_path)? {
        envs.push(("GIT_SSH_COMMAND", command));
    }

    run_git_command(vec!["fetch", "origin"], Some(path), envs)?;
    Ok(())
}

pub fn get_remote_url(repo: &Repository, name: &str) -> Result<String, String> {
    let remote = repo
        .find_remote(name)
        .map_err(|e| format!("Failed to find remote: {}", e))?;
    Ok(remote.url().unwrap_or("").to_string())
}

pub fn set_remote_url(repo: &Repository, name: &str, url: &str) -> Result<(), String> {
    repo.remote_set_url(name, url)
        .map_err(|e| format!("Failed to set remote URL: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::symlink;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};

    fn get_temp_dir() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let mut path = std::env::temp_dir();
        path.push("tauri_git_test");
        path.push(format!(
            "{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn configure_git_identity(path: &std::path::Path) {
        run_git_command(
            vec!["config", "user.name", "Test User"],
            Some(path.to_str().unwrap()),
            vec![],
        )
        .unwrap();
        run_git_command(
            vec!["config", "user.email", "test@example.com"],
            Some(path.to_str().unwrap()),
            vec![],
        )
        .unwrap();
    }

    fn create_dummy_key(dir: &std::path::Path, file_name: &str) -> PathBuf {
        let key_path = dir.join(file_name);
        fs::write(&key_path, "dummy-private-key").unwrap();
        key_path
    }

    fn init_bare_remote(path: &std::path::Path) {
        run_git_command(vec!["init", "--bare"], Some(path.to_str().unwrap()), vec![]).unwrap();
    }

    fn init_working_repo(path: &std::path::Path) -> Repository {
        let repo = Repository::init(path).unwrap();
        configure_git_identity(path);
        repo
    }

    fn commit_file(path: &std::path::Path, name: &str, contents: &str, message: &str) {
        fs::write(path.join(name), contents).unwrap();
        run_git_command(vec!["add", name], Some(path.to_str().unwrap()), vec![]).unwrap();
        run_git_command(
            vec!["commit", "-m", message],
            Some(path.to_str().unwrap()),
            vec![],
        )
        .unwrap();
    }

    fn write_file(path: &Path, name: &str, contents: &str) {
        fs::write(path.join(name), contents).unwrap();
    }

    fn append_file(path: &Path, name: &str, contents: &str) {
        let target = path.join(name);
        let existing = fs::read_to_string(&target).unwrap_or_default();
        fs::write(target, format!("{existing}{contents}")).unwrap();
    }

    fn stage_all_changes(repo: &Repository) {
        let paths = get_status(repo)
            .unwrap()
            .into_iter()
            .map(|status| status.path)
            .collect::<Vec<_>>();
        let result = stage_files(repo, paths).unwrap();
        assert!(
            result.warnings.is_empty(),
            "unexpected staging warnings: {:?}",
            result.warnings
        );
    }

    fn current_branch_name(repo: &Repository) -> String {
        repo.head().unwrap().shorthand().unwrap().to_string()
    }

    fn create_committed_repo() -> (PathBuf, Repository, String) {
        let root = get_temp_dir();
        let repo = init_working_repo(&root);
        commit_file(&root, "base.txt", "base\n", "Initial commit");
        let branch = current_branch_name(&repo);
        (root, repo, branch)
    }

    fn create_merge_conflict_repo() -> (PathBuf, Repository, String) {
        let (root, repo, default_branch) = create_committed_repo();

        run_git_command(
            vec!["checkout", "-b", "feature/conflict"],
            Some(root.to_str().unwrap()),
            vec![],
        )
        .unwrap();
        write_file(&root, "base.txt", "feature version\n");
        stage_all_changes(&repo);
        create_commit(&repo, "Feature change").unwrap();

        run_git_command(
            vec!["checkout", &default_branch],
            Some(root.to_str().unwrap()),
            vec![],
        )
        .unwrap();
        write_file(&root, "base.txt", "main version\n");
        stage_all_changes(&repo);
        create_commit(&repo, "Main change").unwrap();

        let merge_result = run_git_command(
            vec!["merge", "feature/conflict"],
            Some(root.to_str().unwrap()),
            vec![],
        );
        assert!(merge_result.is_err(), "merge should produce a conflict");

        drop(repo);
        let conflicted_repo = Repository::open(&root).unwrap();

        (root, conflicted_repo, default_branch)
    }

    #[test]
    fn test_stage_files_rejects_path_traversal_outside_repository() {
        let root = get_temp_dir();
        let sandbox_root = root.join("sandbox");
        let repo_path = sandbox_root.join("level1/level2/repo");
        let escaped_dir = sandbox_root.join("etc");
        fs::create_dir_all(&repo_path).unwrap();
        fs::create_dir_all(&escaped_dir).unwrap();
        fs::write(escaped_dir.join("passwd"), "not-a-real-passwd").unwrap();

        let repo = init_working_repo(&repo_path);
        commit_file(&repo_path, "tracked.txt", "tracked", "Initial commit");

        let result = stage_files(&repo, vec!["../../../etc/passwd".to_string()]).unwrap();

        assert!(
            result.staged.is_empty(),
            "path traversal should not stage files outside the repository"
        );
        assert_eq!(result.warnings.len(), 1);
        assert!(
            result.warnings[0].contains("Path traversal is not allowed"),
            "unexpected warning: {}",
            result.warnings[0]
        );

        let statuses = get_status(&repo).unwrap();
        assert!(!statuses.iter().any(|status| status.path.contains("passwd")));
        assert_eq!(
            fs::read_to_string(escaped_dir.join("passwd")).unwrap(),
            "not-a-real-passwd"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_discard_changes_ignores_absolute_path_outside_repo() {
        let root = get_temp_dir();
        let repo_path = root.join("repo");
        let outside_path = root.join("outside.txt");
        let repo = init_working_repo(&repo_path);
        commit_file(&repo_path, "tracked.txt", "tracked", "Initial commit");
        fs::write(&outside_path, "top-secret").unwrap();

        let err = discard_changes(&repo, outside_path.to_str().unwrap())
            .expect_err("absolute paths outside the repository should be rejected");

        assert!(
            err.contains("Absolute paths are not allowed"),
            "unexpected error: {err}"
        );

        assert!(
            outside_path.exists(),
            "absolute paths should not delete files outside the repository"
        );
        assert_eq!(fs::read_to_string(&outside_path).unwrap(), "top-secret");

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn test_unstage_files_on_symlink_does_not_modify_symlink_target() {
        let root = get_temp_dir();
        let repo_path = root.join("repo");
        let repo = init_working_repo(&repo_path);
        commit_file(&repo_path, "tracked.txt", "tracked", "Initial commit");

        let outside_target = root.join("secret.txt");
        fs::write(&outside_target, "classified").unwrap();
        let link_path = repo_path.join("linked-secret.txt");
        symlink(&outside_target, &link_path).unwrap();

        let stage_result = stage_files(&repo, vec!["linked-secret.txt".to_string()]).unwrap();
        assert!(stage_result.staged.is_empty());
        assert_eq!(stage_result.warnings.len(), 1);
        assert!(
            stage_result.warnings[0].contains("outside the repository"),
            "unexpected warning: {}",
            stage_result.warnings[0]
        );

        let err = unstage_files(&repo, vec!["linked-secret.txt".to_string()])
            .expect_err("symlinks escaping the repository should be rejected");
        assert!(
            err.contains("outside the repository"),
            "unexpected error: {err}"
        );

        assert_eq!(fs::read_to_string(&outside_target).unwrap(), "classified");
        assert!(
            link_path.exists(),
            "unstage should not remove the symlink itself"
        );

        let statuses = get_status(&repo).unwrap();
        assert!(statuses
            .iter()
            .any(|status| status.path == "linked-secret.txt" && !status.staged));

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn test_stage_files_rejects_new_file_inside_symlinked_parent_outside_repo() {
        let root = get_temp_dir();
        let repo_path = root.join("repo");
        let outside_dir = root.join("outside-dir");
        fs::create_dir_all(&outside_dir).unwrap();

        let repo = init_working_repo(&repo_path);
        commit_file(&repo_path, "tracked.txt", "tracked", "Initial commit");

        let linked_dir = repo_path.join("linked-dir");
        symlink(&outside_dir, &linked_dir).unwrap();

        let result = stage_files(&repo, vec!["linked-dir/new.txt".to_string()]).unwrap();

        assert!(result.staged.is_empty());
        assert_eq!(result.warnings.len(), 1);
        assert!(
            result.warnings[0].contains("outside the repository"),
            "unexpected warning: {}",
            result.warnings[0]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn test_discard_all_changes_rejects_symlinked_untracked_path_outside_repo() {
        let root = get_temp_dir();
        let repo_path = root.join("repo");
        let outside_target = root.join("outside.txt");
        let repo = init_working_repo(&repo_path);
        commit_file(&repo_path, "tracked.txt", "tracked", "Initial commit");
        fs::write(&outside_target, "classified").unwrap();

        symlink(&outside_target, repo_path.join("linked-secret.txt")).unwrap();

        let err = discard_all_changes(&repo)
            .expect_err("discard_all_changes should reject paths escaping the repository");

        assert!(
            err.contains("outside the repository"),
            "unexpected error: {err}"
        );
        assert_eq!(fs::read_to_string(&outside_target).unwrap(), "classified");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_is_safe_git_arg_blocks_common_injection_tokens() {
        for value in [
            "",
            "-branch",
            "feature branch",
            "feature;branch",
            "feature|branch",
            "feature&&branch",
            "feature`branch",
            "feature$branch",
            r"feature\\branch",
        ] {
            assert!(!is_safe_git_arg(value), "expected '{value}' to be rejected");
        }
    }

    #[test]
    fn test_is_safe_git_arg_allows_newlines_and_bidi_controls_currently() {
        assert!(
            is_safe_git_arg("feature\nname"),
            "current validator unexpectedly started rejecting newlines"
        );
        assert!(
            is_safe_git_arg("feature\u{202e}name"),
            "current validator unexpectedly started rejecting bidi control characters"
        );
    }

    #[test]
    fn test_create_and_checkout_branch_reject_special_character_injection_attempts() {
        let root = get_temp_dir();
        let repo = init_working_repo(&root);
        commit_file(&root, "tracked.txt", "tracked", "Initial commit");

        for value in ["feature;rm", "feature|cat", "feature&&cat"] {
            let create_err = create_branch(&repo, value, None).unwrap_err();
            assert_eq!(create_err, "Invalid branch name");

            let checkout_err = checkout_branch(&repo, value).unwrap_err();
            assert_eq!(checkout_err, "Invalid branch name");
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_clone_repository_rejects_url_injection_attempts() {
        let root = get_temp_dir();
        let clone_path = root.join("clone");

        for url in [
            "ssh://example.com/repo.git;touch-pwned",
            "https://example.com/repo.git injected",
            "--upload-pack=sh",
        ] {
            let err = match clone_repository(url, clone_path.to_str().unwrap(), None, None) {
                Ok(_) => panic!("expected invalid clone URL to be rejected: {url}"),
                Err(err) => err,
            };
            assert_eq!(err, "Invalid clone URL");
        }

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_create_commit_treats_injection_like_commit_messages_as_literal_text() {
        let root = get_temp_dir();
        let repo = init_working_repo(&root);
        fs::write(root.join("tracked.txt"), "payload").unwrap();

        let stage_result = stage_files(&repo, vec!["tracked.txt".to_string()]).unwrap();
        assert_eq!(stage_result.staged, vec!["tracked.txt".to_string()]);

        let message = "ship feature && touch /tmp/pwned ; echo nope";
        create_commit(&repo, message).unwrap();

        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message().unwrap(), message);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_create_branch_rejects_invalid_and_nonexistent_start_sha_values() {
        let root = get_temp_dir();
        let repo = init_working_repo(&root);
        commit_file(&root, "tracked.txt", "tracked", "Initial commit");

        let invalid_sha_err = create_branch(&repo, "from-invalid-sha", Some("not-a-sha"))
            .expect_err("invalid SHA should be rejected");
        assert!(
            invalid_sha_err.to_lowercase().contains("oid")
                || invalid_sha_err.to_lowercase().contains("invalid")
                || invalid_sha_err.to_lowercase().contains("length")
        );

        let missing_sha = "0123456789012345678901234567890123456789";
        let missing_sha_err = create_branch(&repo, "from-missing-sha", Some(missing_sha))
            .expect_err("nonexistent SHA should be rejected");
        assert!(missing_sha_err.contains("Commit not found"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_get_commit_diff_rejects_invalid_and_nonexistent_sha_values() {
        let root = get_temp_dir();
        let repo = init_working_repo(&root);
        commit_file(&root, "tracked.txt", "tracked", "Initial commit");

        let invalid_sha_err = get_commit_diff(&repo, "not-a-sha").unwrap_err();
        assert!(
            invalid_sha_err.to_lowercase().contains("oid")
                || invalid_sha_err.to_lowercase().contains("invalid")
                || invalid_sha_err.to_lowercase().contains("length")
        );

        let missing_sha = "0123456789012345678901234567890123456789";
        let missing_sha_err = get_commit_diff(&repo, missing_sha).unwrap_err();
        assert!(missing_sha_err.contains("Commit not found"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_create_branch_with_extremely_long_name_is_rejected() {
        let root = get_temp_dir();
        let repo = init_working_repo(&root);
        commit_file(&root, "tracked.txt", "tracked", "Initial commit");

        let long_name = format!("feature-{}", "a".repeat(512));
        let err = create_branch(&repo, &long_name, None).unwrap_err();
        assert!(
            err == "Invalid branch name" || err.contains("Failed to create branch"),
            "unexpected error: {err}"
        );

        let _ = fs::remove_dir_all(root);
    }

    fn set_bare_remote_head(path: &std::path::Path, branch_name: &str) {
        run_git_command(
            vec!["symbolic-ref", "HEAD", &format!("refs/heads/{branch_name}")],
            Some(path.to_str().unwrap()),
            vec![],
        )
        .unwrap();
    }

    fn head_oid(repo: &Repository) -> git2::Oid {
        repo.head().unwrap().target().unwrap()
    }

    fn safety_ref_names(repo: &Repository, action_name: &str) -> Vec<String> {
        let pattern = format!("refs/safety/{action_name}/*");
        let refs = repo.references_glob(&pattern).unwrap();
        refs.filter_map(|reference| {
            reference
                .ok()
                .and_then(|reference| reference.name().map(|name| name.to_string()))
        })
        .collect()
    }

    fn single_safety_ref_name(repo: &Repository, action_name: &str) -> String {
        let refs = safety_ref_names(repo, action_name);
        assert_eq!(
            refs.len(),
            1,
            "expected exactly one safety ref for {action_name}"
        );
        refs[0].clone()
    }

    fn hard_reset_to_ref(repo: &Repository, ref_name: &str) {
        let target = repo.revparse_single(ref_name).unwrap();
        repo.reset(&target, git2::ResetType::Hard, None).unwrap();
    }

    fn assert_repo_is_clean(repo: &Repository) {
        let statuses = repo.statuses(None).unwrap();
        assert!(statuses.is_empty(), "repository should be clean");
    }

    #[test]
    fn test_build_git_ssh_command_returns_none_for_missing_or_blank_key() {
        assert_eq!(build_git_ssh_command(None).unwrap(), None);
        assert_eq!(build_git_ssh_command(Some("   ")).unwrap(), None);
    }

    #[test]
    fn test_build_git_ssh_command_expands_home_and_shell_escapes_key_path() {
        let _lock = env_lock().lock().unwrap();
        let root = get_temp_dir();
        let home_dir = root.join("home");
        let ssh_dir = home_dir.join(".ssh");
        fs::create_dir_all(&ssh_dir).unwrap();

        let key_name = "team member's key";
        let key_path = create_dummy_key(&ssh_dir, key_name);

        let previous_home = std::env::var_os("HOME");
        std::env::set_var("HOME", &home_dir);

        let command = build_git_ssh_command(Some("~/.ssh/team member's key"))
            .unwrap()
            .expect("ssh command should be built");

        if let Some(home) = previous_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }

        assert!(command.starts_with("ssh -i '") && command.ends_with(" -o IdentitiesOnly=yes"));
        assert!(command.contains(home_dir.to_string_lossy().as_ref()));
        assert!(command.contains("team member"));
        assert!(command.contains("'\\''s key"));
        assert!(key_path.exists());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_build_git_ssh_command_rejects_missing_key_file() {
        let missing_key = get_temp_dir().join("missing-key");
        let err = build_git_ssh_command(Some(missing_key.to_str().unwrap())).unwrap_err();
        assert!(
            err.contains("SSH key file does not exist"),
            "unexpected error: {err}"
        );
        let _ = fs::remove_dir_all(missing_key.parent().unwrap());
    }

    #[test]
    fn test_clone_repository_rejects_missing_ssh_key_file() {
        let root = get_temp_dir();
        let origin_path = root.join("origin.git");
        let clone_path = root.join("clone");
        fs::create_dir_all(&origin_path).unwrap();
        init_bare_remote(&origin_path);

        let missing_key = root.join("missing-key");
        let err = match clone_repository(
            origin_path.to_str().unwrap(),
            clone_path.to_str().unwrap(),
            Some(missing_key.to_str().unwrap()),
            Some("secret-passphrase"),
        ) {
            Ok(_) => panic!("clone_repository unexpectedly succeeded"),
            Err(err) => err,
        };

        assert!(
            err.contains("SSH key file does not exist"),
            "unexpected error: {err}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_clone_repository_supports_safe_ssh_key_path_for_local_transport() {
        let root = get_temp_dir();
        let seed_path = root.join("seed");
        let origin_path = root.join("origin.git");
        let clone_path = root.join("clone");
        let key_path = create_dummy_key(&root, "enterprise team's key");

        let seed_repo = init_working_repo(&seed_path);
        commit_file(&seed_path, "README.md", "seed", "Initial commit");

        fs::create_dir_all(&origin_path).unwrap();
        init_bare_remote(&origin_path);
        run_git_command(
            vec!["remote", "add", "origin", origin_path.to_str().unwrap()],
            Some(seed_path.to_str().unwrap()),
            vec![],
        )
        .unwrap();
        push_changes(
            &seed_repo,
            Some(key_path.to_str().unwrap()),
            Some("ignored-passphrase"),
        )
        .unwrap();
        let branch_name = seed_repo.head().unwrap().shorthand().unwrap().to_string();
        set_bare_remote_head(&origin_path, &branch_name);

        let clone_repo = clone_repository(
            origin_path.to_str().unwrap(),
            clone_path.to_str().unwrap(),
            Some(key_path.to_str().unwrap()),
            Some("ignored-passphrase"),
        )
        .unwrap();

        let head = clone_repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message().unwrap().trim(), "Initial commit");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_push_and_fetch_changes_support_safe_ssh_key_path() {
        let root = get_temp_dir();
        let origin_path = root.join("origin.git");
        let local_path = root.join("local");
        let peer_path = root.join("peer");
        let key_path = create_dummy_key(&root, "ops team's key");

        fs::create_dir_all(&origin_path).unwrap();
        init_bare_remote(&origin_path);

        let local_repo = init_working_repo(&local_path);
        run_git_command(
            vec!["remote", "add", "origin", origin_path.to_str().unwrap()],
            Some(local_path.to_str().unwrap()),
            vec![],
        )
        .unwrap();

        commit_file(&local_path, "push.txt", "v1", "Initial push");
        push_changes(
            &local_repo,
            Some(key_path.to_str().unwrap()),
            Some("ignored-passphrase"),
        )
        .unwrap();

        run_git_command(
            vec![
                "clone",
                origin_path.to_str().unwrap(),
                peer_path.to_str().unwrap(),
            ],
            None,
            vec![],
        )
        .unwrap();
        configure_git_identity(&peer_path);
        let peer_repo = Repository::open(&peer_path).unwrap();

        fetch_changes(
            &peer_repo,
            Some(key_path.to_str().unwrap()),
            Some("ignored-passphrase"),
        )
        .unwrap();

        let fetch_head = peer_repo.find_reference("refs/remotes/origin/HEAD");
        assert!(
            fetch_head.is_ok()
                || peer_repo
                    .find_reference("refs/remotes/origin/master")
                    .is_ok()
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_pull_changes_supports_safe_ssh_key_path() {
        let root = get_temp_dir();
        let origin_path = root.join("origin");
        let local_path = root.join("local");
        let key_path = create_dummy_key(&root, "release team's key");

        fs::create_dir_all(&origin_path).unwrap();
        let _ = Repository::init(&origin_path).unwrap();
        configure_git_identity(&origin_path);
        run_git_command(
            vec!["commit", "--allow-empty", "-m", "Initial commit"],
            Some(origin_path.to_str().unwrap()),
            vec![],
        )
        .unwrap();

        run_git_command(
            vec![
                "clone",
                origin_path.to_str().unwrap(),
                local_path.to_str().unwrap(),
            ],
            None,
            vec![],
        )
        .unwrap();
        configure_git_identity(&local_path);
        let local_repo = Repository::open(&local_path).unwrap();

        commit_file(&origin_path, "new_file.txt", "content", "Feature commit");

        pull_changes(
            &local_repo,
            Some(key_path.to_str().unwrap()),
            Some("ignored-passphrase"),
        )
        .unwrap();

        let head = local_repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message().unwrap().trim(), "Feature commit");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_pull_changes() {
        // Setup origin
        let root = get_temp_dir();
        let origin_path = root.join("origin");
        let local_path = root.join("local");

        fs::create_dir(&origin_path).unwrap();
        let _ = Repository::init(&origin_path).unwrap();

        // Initial commit in origin
        run_git_command(vec!["init"], Some(origin_path.to_str().unwrap()), vec![]).unwrap();
        run_git_command(
            vec!["config", "user.name", "Test User"],
            Some(origin_path.to_str().unwrap()),
            vec![],
        )
        .unwrap();
        run_git_command(
            vec!["config", "user.email", "test@example.com"],
            Some(origin_path.to_str().unwrap()),
            vec![],
        )
        .unwrap();
        run_git_command(
            vec!["commit", "--allow-empty", "-m", "Initial commit"],
            Some(origin_path.to_str().unwrap()),
            vec![],
        )
        .unwrap();

        // Create a branch 'feature'
        run_git_command(
            vec!["checkout", "-b", "feature"],
            Some(origin_path.to_str().unwrap()),
            vec![],
        )
        .unwrap();

        // Clone to local
        run_git_command(
            vec![
                "clone",
                origin_path.to_str().unwrap(),
                local_path.to_str().unwrap(),
            ],
            None,
            vec![],
        )
        .unwrap();
        let local = Repository::open(&local_path).unwrap();
        run_git_command(
            vec!["config", "user.name", "Test User"],
            Some(local_path.to_str().unwrap()),
            vec![],
        )
        .unwrap();
        run_git_command(
            vec!["config", "user.email", "test@example.com"],
            Some(local_path.to_str().unwrap()),
            vec![],
        )
        .unwrap();

        // Switch local to feature branch (needs fetch first usually but clone gets all)
        // Checkout feature branch tracking origin/feature
        // origin was on 'feature', so clone checked it out. We just ensure we are on it.
        let _ = run_git_command(
            vec!["checkout", "feature"],
            Some(local_path.to_str().unwrap()),
            vec![],
        );

        // Add commit to origin/feature
        let file_path = origin_path.join("new_file.txt");
        fs::write(&file_path, "content").unwrap();
        run_git_command(
            vec!["add", "new_file.txt"],
            Some(origin_path.to_str().unwrap()),
            vec![],
        )
        .unwrap();
        run_git_command(
            vec!["commit", "-m", "Feature commit"],
            Some(origin_path.to_str().unwrap()),
            vec![],
        )
        .unwrap();

        // Run pull_changes
        let result = pull_changes(&local, None, None);
        assert!(result.is_ok(), "pull_changes failed: {:?}", result.err());

        // Verify local has the commit
        let head = local.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        assert_eq!(commit.message().unwrap().trim(), "Feature commit");

        // Cleanup
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_amend_commit() {
        let root = get_temp_dir();
        let _ = Repository::init(&root).unwrap();
        let repo = Repository::open(&root).unwrap();

        run_git_command(
            vec!["config", "user.name", "Test User"],
            Some(root.to_str().unwrap()),
            vec![],
        )
        .unwrap();
        run_git_command(
            vec!["config", "user.email", "test@example.com"],
            Some(root.to_str().unwrap()),
            vec![],
        )
        .unwrap();

        // Initial commit
        let file_path = root.join("file.txt");
        fs::write(&file_path, "v1").unwrap();
        run_git_command(vec!["add", "."], Some(root.to_str().unwrap()), vec![]).unwrap();
        create_commit(&repo, "Initial commit").unwrap();

        // Amend
        let result = amend_last_commit(&repo, "Amended message");
        assert!(result.is_ok());

        let head = repo.head().unwrap();
        let commit = head.peel_to_commit().unwrap();
        assert_eq!(commit.message().unwrap(), "Amended message");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_amend_commit_creates_safety_ref_and_supports_manual_restore() {
        let root = get_temp_dir();
        let repo = init_working_repo(&root);

        commit_file(&root, "file.txt", "v1", "Initial commit");
        let original_head = head_oid(&repo);

        fs::write(root.join("file.txt"), "v2").unwrap();
        run_git_command(
            vec!["add", "file.txt"],
            Some(root.to_str().unwrap()),
            vec![],
        )
        .unwrap();

        amend_last_commit(&repo, "Amended commit").unwrap();

        let safety_ref = single_safety_ref_name(&repo, "amend");
        assert_eq!(
            repo.find_reference(&safety_ref).unwrap().target().unwrap(),
            original_head
        );
        assert_ne!(head_oid(&repo), original_head);

        hard_reset_to_ref(&repo, &safety_ref);

        assert_eq!(head_oid(&repo), original_head);
        assert_eq!(fs::read_to_string(root.join("file.txt")).unwrap(), "v1");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_cherry_pick_creates_safety_ref_and_cleans_up_state_on_success() {
        let root = get_temp_dir();
        let repo = init_working_repo(&root);

        commit_file(&root, "shared.txt", "base\n", "Base commit");
        let main_branch = repo.head().unwrap().shorthand().unwrap().to_string();

        run_git_command(
            vec!["checkout", "-b", "feature"],
            Some(root.to_str().unwrap()),
            vec![],
        )
        .unwrap();
        commit_file(&root, "feature.txt", "picked\n", "Feature commit");
        let picked_sha = head_oid(&repo).to_string();

        run_git_command(
            vec!["checkout", &main_branch],
            Some(root.to_str().unwrap()),
            vec![],
        )
        .unwrap();
        let pre_cherry_pick_head = head_oid(&repo);

        cherry_pick(&repo, &picked_sha).unwrap();

        let safety_ref = single_safety_ref_name(&repo, "cherry-pick");
        assert_eq!(
            repo.find_reference(&safety_ref).unwrap().target().unwrap(),
            pre_cherry_pick_head
        );
        assert!(!repo.path().join("CHERRY_PICK_HEAD").exists());
        assert_eq!(
            fs::read_to_string(root.join("feature.txt")).unwrap(),
            "picked\n"
        );
        assert_repo_is_clean(&repo);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_cherry_pick_conflict_preserves_state_until_manual_recovery() {
        let root = get_temp_dir();
        let repo = init_working_repo(&root);

        commit_file(&root, "shared.txt", "base\n", "Base commit");
        let main_branch = repo.head().unwrap().shorthand().unwrap().to_string();

        run_git_command(
            vec!["checkout", "-b", "feature"],
            Some(root.to_str().unwrap()),
            vec![],
        )
        .unwrap();
        commit_file(&root, "shared.txt", "feature\n", "Feature change");
        let picked_sha = head_oid(&repo).to_string();

        run_git_command(
            vec!["checkout", &main_branch],
            Some(root.to_str().unwrap()),
            vec![],
        )
        .unwrap();
        commit_file(&root, "shared.txt", "main\n", "Main change");
        let pre_cherry_pick_head = head_oid(&repo);

        let err = cherry_pick(&repo, &picked_sha).unwrap_err();
        assert!(err.contains("conflicts"), "unexpected error: {err}");

        let safety_ref = single_safety_ref_name(&repo, "cherry-pick");
        assert_eq!(
            repo.find_reference(&safety_ref).unwrap().target().unwrap(),
            pre_cherry_pick_head
        );
        assert!(repo.path().join("CHERRY_PICK_HEAD").exists());
        assert!(repo.index().unwrap().has_conflicts());

        hard_reset_to_ref(&repo, &safety_ref);
        repo.cleanup_state().unwrap();

        assert_eq!(head_oid(&repo), pre_cherry_pick_head);
        assert!(!repo.path().join("CHERRY_PICK_HEAD").exists());
        assert_eq!(
            fs::read_to_string(root.join("shared.txt")).unwrap(),
            "main\n"
        );
        assert_repo_is_clean(&repo);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_revert_commit_creates_safety_ref_and_cleans_up_state_on_success() {
        let root = get_temp_dir();
        let repo = init_working_repo(&root);

        commit_file(&root, "shared.txt", "v1\n", "Base commit");
        commit_file(&root, "shared.txt", "v2\n", "Second commit");
        let commit_to_revert = head_oid(&repo).to_string();
        let pre_revert_head = head_oid(&repo);

        revert_commit(&repo, &commit_to_revert).unwrap();

        let safety_ref = single_safety_ref_name(&repo, "revert");
        assert_eq!(
            repo.find_reference(&safety_ref).unwrap().target().unwrap(),
            pre_revert_head
        );
        assert!(!repo.path().join("REVERT_HEAD").exists());
        assert_eq!(fs::read_to_string(root.join("shared.txt")).unwrap(), "v1\n");
        assert_repo_is_clean(&repo);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_revert_conflict_preserves_state_until_manual_recovery() {
        let root = get_temp_dir();
        let repo = init_working_repo(&root);

        commit_file(&root, "shared.txt", "base\n", "Base commit");
        commit_file(&root, "shared.txt", "feature\n", "Feature commit");
        let commit_to_revert = head_oid(&repo).to_string();
        commit_file(&root, "shared.txt", "main\n", "Main commit");
        let pre_revert_head = head_oid(&repo);

        let err = revert_commit(&repo, &commit_to_revert).unwrap_err();
        assert!(err.contains("conflicts"), "unexpected error: {err}");

        let safety_ref = single_safety_ref_name(&repo, "revert");
        assert_eq!(
            repo.find_reference(&safety_ref).unwrap().target().unwrap(),
            pre_revert_head
        );
        assert!(repo.path().join("REVERT_HEAD").exists());
        assert!(repo.index().unwrap().has_conflicts());

        hard_reset_to_ref(&repo, &safety_ref);
        repo.cleanup_state().unwrap();

        assert_eq!(head_oid(&repo), pre_revert_head);
        assert!(!repo.path().join("REVERT_HEAD").exists());
        assert_eq!(
            fs::read_to_string(root.join("shared.txt")).unwrap(),
            "main\n"
        );
        assert_repo_is_clean(&repo);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_clone_repository_failure_does_not_leave_openable_destination() {
        let root = get_temp_dir();
        let missing_origin = root.join("missing-origin.git");
        let clone_path = root.join("clone");

        let err = match clone_repository(
            missing_origin.to_str().unwrap(),
            clone_path.to_str().unwrap(),
            None,
            None,
        ) {
            Ok(_) => panic!("clone_repository unexpectedly succeeded"),
            Err(err) => err,
        };

        assert!(
            err.contains("does not exist")
                || err.contains("repository")
                || err.contains("not found"),
            "unexpected error: {err}"
        );
        assert!(open_repository(clone_path.to_str().unwrap()).is_err());
        assert!(!clone_path.join(".git").exists());

        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn test_clone_repository_reports_permission_denied_for_unwritable_destination() {
        use std::os::unix::fs::PermissionsExt;

        let root = get_temp_dir();
        let origin_path = root.join("origin.git");
        let unwritable_parent = root.join("unwritable");
        let clone_path = unwritable_parent.join("clone");

        fs::create_dir_all(&origin_path).unwrap();
        init_bare_remote(&origin_path);
        fs::create_dir_all(&unwritable_parent).unwrap();

        let original_permissions = fs::metadata(&unwritable_parent).unwrap().permissions();
        let mut restricted_permissions = original_permissions.clone();
        restricted_permissions.set_mode(0o500);
        fs::set_permissions(&unwritable_parent, restricted_permissions).unwrap();

        let err = match clone_repository(
            origin_path.to_str().unwrap(),
            clone_path.to_str().unwrap(),
            None,
            None,
        ) {
            Ok(_) => panic!("clone_repository unexpectedly succeeded"),
            Err(err) => err,
        };

        let mut restored_permissions = fs::metadata(&unwritable_parent).unwrap().permissions();
        restored_permissions.set_mode(original_permissions.mode());
        fs::set_permissions(&unwritable_parent, restored_permissions).unwrap();

        assert!(
            err.contains("Permission denied") || err.contains("could not create work tree dir"),
            "unexpected error: {err}"
        );
        assert!(open_repository(clone_path.to_str().unwrap()).is_err());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_open_repository_reports_invalid_path() {
        let missing_path = get_temp_dir().join("missing-repo");
        let err = match open_repository(missing_path.to_str().unwrap()) {
            Ok(_) => panic!("open_repository unexpectedly succeeded"),
            Err(err) => err,
        };

        assert!(
            err.contains("Failed to open repository"),
            "unexpected error: {err}"
        );

        let _ = fs::remove_dir_all(missing_path.parent().unwrap());
    }

    #[test]
    fn test_discard_all_changes() {
        let root = get_temp_dir();
        let _ = Repository::init(&root).unwrap();
        let repo = Repository::open(&root).unwrap();

        run_git_command(
            vec!["config", "user.name", "Test User"],
            Some(root.to_str().unwrap()),
            vec![],
        )
        .unwrap();
        run_git_command(
            vec!["config", "user.email", "test@example.com"],
            Some(root.to_str().unwrap()),
            vec![],
        )
        .unwrap();

        fs::write(root.join("file.txt"), "v1").unwrap();
        run_git_command(vec!["add", "."], Some(root.to_str().unwrap()), vec![]).unwrap();
        create_commit(&repo, "Init").unwrap();

        // Modify file
        fs::write(root.join("file.txt"), "v2").unwrap();

        // Discard
        discard_all_changes(&repo).unwrap();

        let content = fs::read_to_string(root.join("file.txt")).unwrap();
        assert_eq!(content, "v1");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_branch_operations_create_checkout_and_list_successfully() {
        let (root, repo, default_branch) = create_committed_repo();

        create_branch(&repo, "release-2026", None).unwrap();
        assert_eq!(current_branch_name(&repo), "release-2026");

        let branches = get_branches(&repo).unwrap();
        assert!(branches.iter().any(|branch| branch.name == default_branch));
        assert!(branches
            .iter()
            .any(|branch| branch.name == "release-2026" && branch.is_current));

        checkout_branch(&repo, &default_branch).unwrap();
        assert_eq!(current_branch_name(&repo), default_branch);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_branch_operations_reject_invalid_inputs_and_missing_targets() {
        let (root, repo, _) = create_committed_repo();

        let invalid_name_error = create_branch(&repo, "bad branch", None).unwrap_err();
        assert!(invalid_name_error.contains("Invalid branch name"));

        let bad_sha_error = create_branch(&repo, "hotfix-1", Some("deadbeef")).unwrap_err();
        assert!(bad_sha_error.contains("Commit not found") || bad_sha_error.contains("too short"));

        let missing_branch_error = checkout_branch(&repo, "missing-branch").unwrap_err();
        assert!(missing_branch_error.contains("Failed to find branch"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_create_commit_history_and_diff_cover_success_and_boundary_cases() {
        let root = get_temp_dir();
        let repo = init_working_repo(&root);

        write_file(&root, "notes.txt", "line one\nline two\n");
        stage_all_changes(&repo);
        let first_sha = create_commit(&repo, "Initial enterprise commit").unwrap();

        let first_history = get_commit_history(&repo, 10).unwrap();
        assert_eq!(first_history.len(), 1);
        assert_eq!(first_history[0].message, "Initial enterprise commit");
        assert!(first_history[0].parents.is_empty());
        assert!(!first_history[0].is_pushed);

        let first_diff = get_commit_diff(&repo, &first_sha).unwrap();
        assert_eq!(first_diff.len(), 1);
        assert_eq!(first_diff[0].path, "notes.txt");
        assert!(first_diff[0].additions >= 2);
        assert!(first_diff[0].diff_text.contains("+line one"));

        append_file(&root, "notes.txt", "line three\n");
        stage_all_changes(&repo);
        let second_sha = create_commit(&repo, "Second enterprise commit").unwrap();

        let limited_history = get_commit_history(&repo, 1).unwrap();
        assert_eq!(limited_history.len(), 1);
        assert_eq!(limited_history[0].sha, second_sha);
        assert_eq!(limited_history[0].message, "Second enterprise commit");
        assert_eq!(limited_history[0].parents.len(), 1);

        let second_diff = get_commit_diff(&repo, &second_sha).unwrap();
        assert_eq!(second_diff.len(), 1);
        assert!(second_diff[0].diff_text.contains("+line three"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_commit_operations_report_errors_for_empty_history_and_invalid_sha() {
        let root = get_temp_dir();
        let repo = init_working_repo(&root);

        let amend_error = amend_last_commit(&repo, "No commit yet").unwrap_err();
        assert!(amend_error.contains("Failed to get HEAD"));

        let history_error = get_commit_history(&repo, 10).unwrap_err();
        assert!(history_error.contains("Failed to push HEAD"));

        let diff_error = get_commit_diff(&repo, "deadbeef").unwrap_err();
        assert!(diff_error.contains("Commit not found") || diff_error.contains("too short"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_create_commit_supports_empty_initial_commit_boundary() {
        let root = get_temp_dir();
        let repo = init_working_repo(&root);

        let commit_sha = create_commit(&repo, "Empty initial commit").unwrap();
        let history = get_commit_history(&repo, 1).unwrap();

        assert_eq!(history[0].sha, commit_sha);
        assert_eq!(history[0].message, "Empty initial commit");
        assert!(get_commit_diff(&repo, &commit_sha).unwrap().is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_stash_operations_save_list_and_pop_tracked_and_untracked_changes() {
        let (root, mut repo, _) = create_committed_repo();

        write_file(&root, "base.txt", "updated tracked content\n");
        write_file(&root, "draft.txt", "untracked draft\n");

        stash_save(&mut repo, Some("enterprise-wip")).unwrap();

        let stashes = stash_list(&mut repo).unwrap();
        assert_eq!(stashes.len(), 1);
        assert!(stashes[0].message.contains("enterprise-wip"));
        assert_eq!(fs::read_to_string(root.join("base.txt")).unwrap(), "base\n");
        assert!(!root.join("draft.txt").exists());

        stash_pop(&mut repo, 0).unwrap();

        assert_eq!(
            fs::read_to_string(root.join("base.txt")).unwrap(),
            "updated tracked content\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("draft.txt")).unwrap(),
            "untracked draft\n"
        );
        assert!(stash_list(&mut repo).unwrap().is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_stash_operations_handle_empty_and_clean_repository_cases() {
        let (root, mut repo, _) = create_committed_repo();

        assert!(stash_list(&mut repo).unwrap().is_empty());

        let stash_error = stash_save(&mut repo, Some("nothing to stash")).unwrap_err();
        assert!(stash_error.contains("Failed to stash"));

        let pop_error = stash_pop(&mut repo, 0).unwrap_err();
        assert!(pop_error.contains("Failed to pop stash"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_conflict_detection_and_resolution_workflow() {
        let (root, repo, _) = create_merge_conflict_repo();

        let conflicts = get_conflicts(&repo).unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].path, "base.txt");
        assert_eq!(conflicts[0].our_status, "modified");
        assert_eq!(conflicts[0].their_status, "modified");

        resolve_conflict(&repo, "base.txt", true).unwrap();

        assert!(get_conflicts(&repo).unwrap().is_empty());
        assert!(!repo.index().unwrap().has_conflicts());
        assert_eq!(
            fs::read_to_string(root.join("base.txt")).unwrap(),
            "main version\n"
        );
        assert!(repo
            .index()
            .unwrap()
            .get_path(Path::new("base.txt"), 0)
            .is_some());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_resolve_conflict_can_choose_theirs_version() {
        let (root, repo, _) = create_merge_conflict_repo();

        resolve_conflict(&repo, "base.txt", false).unwrap();

        assert!(get_conflicts(&repo).unwrap().is_empty());
        assert!(!repo.index().unwrap().has_conflicts());
        assert_eq!(
            fs::read_to_string(root.join("base.txt")).unwrap(),
            "feature version\n"
        );
        assert!(repo
            .index()
            .unwrap()
            .get_path(Path::new("base.txt"), 0)
            .is_some());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_conflict_operations_report_empty_and_missing_path_cases() {
        let (root, repo, _) = create_committed_repo();

        assert!(get_conflicts(&repo).unwrap().is_empty());

        let resolve_error = resolve_conflict(&repo, "missing.txt", true).unwrap_err();
        assert!(resolve_error.contains("Failed to resolve"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_cherry_pick_success_and_error_cases() {
        let (root, repo, default_branch) = create_committed_repo();

        run_git_command(
            vec!["checkout", "-b", "feature/cherry-pick"],
            Some(root.to_str().unwrap()),
            vec![],
        )
        .unwrap();
        write_file(&root, "picked.txt", "picked change\n");
        stage_all_changes(&repo);
        let picked_sha = create_commit(&repo, "Cherry target commit").unwrap();

        run_git_command(
            vec!["checkout", &default_branch],
            Some(root.to_str().unwrap()),
            vec![],
        )
        .unwrap();

        cherry_pick(&repo, &picked_sha).unwrap();
        assert_eq!(
            repo.head()
                .unwrap()
                .peel_to_commit()
                .unwrap()
                .message()
                .unwrap(),
            "Cherry target commit"
        );
        assert_eq!(
            fs::read_to_string(root.join("picked.txt")).unwrap(),
            "picked change\n"
        );

        let invalid_sha_error = cherry_pick(&repo, "deadbeef").unwrap_err();
        assert!(
            invalid_sha_error.contains("Commit not found")
                || invalid_sha_error.contains("too short")
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn test_revert_commit_success_and_error_cases() {
        let (root, repo, _) = create_committed_repo();

        write_file(&root, "base.txt", "version two\n");
        stage_all_changes(&repo);
        let change_sha = create_commit(&repo, "Promote version two").unwrap();

        revert_commit(&repo, &change_sha).unwrap();

        let head = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head.message().unwrap(), "Revert \"Promote version two\"");
        assert_eq!(fs::read_to_string(root.join("base.txt")).unwrap(), "base\n");

        let invalid_sha_error = revert_commit(&repo, "deadbeef").unwrap_err();
        assert!(
            invalid_sha_error.contains("Commit not found")
                || invalid_sha_error.contains("too short")
        );

        let _ = fs::remove_dir_all(root);
    }
}
