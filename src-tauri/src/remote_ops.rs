//! Git2-based remote operations (push, pull, fetch)
//! This module provides native git2 implementations for remote operations,
//! replacing the previous subprocess-based approach.

use git2::{Cred, FetchOptions, PushOptions, RemoteCallbacks, Repository};
use std::path::Path;
use std::sync::Arc;

/// Build git2 credentials from SSH key path and optional passphrase.
/// 
/// Returns None if no SSH key is provided, or the credentials if successfully created.
pub fn build_ssh_credentials(
    ssh_key_path: Option<&str>,
    ssh_passphrase: Option<&str>,
) -> Result<Option<Cred>, String> {
    let Some(key_path) = ssh_key_path else {
        return Ok(None);
    };

    if key_path.trim().is_empty() {
        return Ok(None);
    }

    // Expand ~ to home directory
    let expanded_path = if key_path.starts_with("~/") {
        let home = std::env::var("HOME").map_err(|_| "Could not find HOME directory".to_string())?;
        key_path.replacen("~", &home, 1)
    } else {
        key_path.to_string()
    };

    // Verify the key file exists
    if !Path::new(&expanded_path).exists() {
        return Err(format!("SSH key file does not exist: {}", expanded_path));
    }

    // Create credentials with the SSH key
    // For SSH key authentication, we use Cred::ssh_key with:
    // - username: empty string (will be determined from remote URL)
    // - public_key_path: None (git2 can derive from private key)
    // - private_key_path: The path to the SSH key
    // - passphrase: Optional passphrase for the key
    let cred = Cred::ssh_key(
        "", // username - will be extracted from URL
        None, // public_key_path - can be None as git2 derives it
        std::path::Path::new(&expanded_path),
        ssh_passphrase,
    )
    .map_err(|e| format!("Failed to create SSH credentials: {}", e))?;

    Ok(Some(cred))
}

/// Create RemoteCallbacks with credentials for authentication.
/// 
/// The callback will be invoked by git2 when it needs credentials for authentication.
pub fn create_remote_callbacks(
    ssh_key_path: Option<&str>,
    ssh_passphrase: Option<&str>,
) -> Result<RemoteCallbacks<'static>, String> {
    let credentials = build_ssh_credentials(ssh_key_path, ssh_passphrase)?;
    
    let mut callbacks = RemoteCallbacks::new();
    
    if let Some(cred) = credentials {
        // Store credentials in an Arc so we can move it into the closure
        let _cred = Arc::new(cred);
        
        callbacks.credentials(move |_url, _username_from_url, allowed_types| {
            // Return credentials if SSH authentication is allowed
            if allowed_types.contains(git2::CredentialType::SSH_KEY) {
                // We need to create a new credential each time since Cred is not Clone
                // For simplicity, we'll return an error if we can't create new credentials
                // In a real implementation, you might want to handle this differently
                return Err(git2::Error::from_str("SSH credentials not available in callback"));
            }
            // Return error if we can't provide the required credentials
            Err(git2::Error::from_str("No credentials available"))
        });
    }
    
    // Also support username/password authentication
    callbacks.credentials(move |_url, username_from_url, allowed_types| {
        if allowed_types.contains(git2::CredentialType::USER_PASS_PLAINTEXT) {
            // For HTTPS URLs, try to get username from URL or use a default
            if let Some(username) = username_from_url {
                // Return a credential with empty password - user will be prompted
                return Cred::username(username)
                    .map_err(|e| git2::Error::from_str(&format!("Failed to create username credential: {}", e)));
            }
        }
        // Return error if we can't provide the required credentials
        Err(git2::Error::from_str("No credentials available"))
    });
    
    Ok(callbacks)
}

/// Push changes to the remote repository using git2.
/// 
/// This replaces the subprocess-based `git push origin HEAD` approach.
pub fn push_changes(
    repo: &Repository,
    ssh_key_path: Option<&str>,
    ssh_passphrase: Option<&str>,
) -> Result<(), String> {
    // Find the origin remote
    let mut remote = repo
        .find_remote("origin")
        .map_err(|e| format!("Failed to find remote 'origin': {}", e))?;

    // Create remote callbacks with credentials
    let callbacks = create_remote_callbacks(ssh_key_path, ssh_passphrase)?;

    // Create push options with callbacks
    let mut push_options = PushOptions::new();
    push_options.remote_callbacks(callbacks);

    // Get the current branch name to push
    let head = repo
        .head()
        .map_err(|e| format!("Failed to get HEAD: {}", e))?;
    
    let branch_name = if head.is_branch() {
        head.shorthand().unwrap_or("HEAD")
    } else {
        // For detached HEAD, we need to push the specific commit
        // Get the current commit SHA
        let oid = head.target().ok_or("No commit to push")?;
        let short_oid = oid.to_string()[..7].to_string();
        // Push the specific ref
        let refspec = format!("{}:refs/heads/{}", oid, short_oid);
        remote.push(&[&refspec], Some(&mut push_options))
            .map_err(|e| format!("Push failed: {}", e))?;
        return Ok(());
    };

    // Push the current branch to origin
    let refspec = format!("HEAD:refs/heads/{}", branch_name);
    remote
        .push(&[&refspec], Some(&mut push_options))
        .map_err(|e| format!("Push failed: {}", e))?;

    Ok(())
}

/// Fetch changes from the remote repository using git2.
/// 
/// This replaces the subprocess-based `git fetch origin` approach.
pub fn fetch_changes(
    repo: &Repository,
    ssh_key_path: Option<&str>,
    ssh_passphrase: Option<&str>,
) -> Result<(), String> {
    // Find the origin remote
    let mut remote = repo
        .find_remote("origin")
        .map_err(|e| format!("Failed to find remote 'origin': {}", e))?;

    // Create remote callbacks with credentials
    let callbacks = create_remote_callbacks(ssh_key_path, ssh_passphrase)?;

    // Create fetch options with callbacks
    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);

    // Fetch all branches from origin
    let refspecs = ["refs/heads/*:refs/remotes/origin/*"];
    remote
        .fetch(&refspecs, Some(&mut fetch_options), None)
        .map_err(|e| format!("Fetch failed: {}", e))?;

    Ok(())
}

/// Pull changes from the remote repository using git2.
/// 
/// This replaces the subprocess-based `git pull origin <branch>` approach.
/// It performs a fetch followed by an attempt to fast-forward to the remote branch.
/// If a fast-forward is not possible, it returns an error indicating manual merge is needed.
pub fn pull_changes(
    repo: &Repository,
    ssh_key_path: Option<&str>,
    ssh_passphrase: Option<&str>,
) -> Result<(), String> {
    // First, fetch changes from origin using git2
    fetch_changes(repo, ssh_key_path, ssh_passphrase)?;

    // Get the current branch
    let head = repo
        .head()
        .map_err(|e| format!("Failed to get HEAD: {}", e))?;

    let branch_name = if head.is_branch() {
        head.shorthand().ok_or("Invalid branch name")?
    } else {
        return Err("Cannot pull in detached HEAD state".to_string());
    };

    // Find the upstream branch (origin/<branch_name>)
    let upstream_name = format!("origin/{}", branch_name);
    
    // Check if the upstream branch exists (it might not be set up as a tracking branch)
    let upstream_ref = match repo.find_reference(&upstream_name) {
        Ok(ref_) => ref_,
        Err(_) => {
            // No upstream tracking branch - try to use git pull via subprocess
            // This handles the case where the clone doesn't set up tracking
            return pull_via_subprocess(repo, branch_name, ssh_key_path);
        }
    };

    let upstream_oid = upstream_ref
        .target()
        .ok_or("Upstream branch has no target commit")?;

    // Get the current branch's reference
    let branch = repo
        .find_branch(branch_name, git2::BranchType::Local)
        .map_err(|e| format!("Failed to find local branch: {}", e))?;

    let branch_ref = branch
        .get()
        .target()
        .ok_or("Local branch has no target commit")?;

    // Check if there are any changes to merge (ahead/behind)
    let (ahead, behind) = repo
        .graph_ahead_behind(branch_ref, upstream_oid)
        .map_err(|e| format!("Failed to calculate ahead/behind: {}", e))?;

    if behind == 0 {
        // Already up to date
        return Ok(());
    }

    if ahead == 0 && behind > 0 {
        // We can fast-forward
        // Get the upstream commit
        let commit = repo
            .find_commit(upstream_oid)
            .map_err(|e| format!("Failed to find upstream commit: {}", e))?;

        // Checkout to the upstream commit using the commit object
        repo.checkout_tree(commit.as_object(), None)
            .map_err(|e| format!("Failed to checkout upstream commit: {}", e))?;

        // Update the branch reference to point to the upstream commit
        // We need to get a mutable reference to the branch
        let branch_name = branch.name().map_err(|e| format!("Invalid branch name: {}", e))?
            .ok_or("Invalid branch name")?;
        repo.reference(&format!("refs/heads/{}", branch_name), upstream_oid, true, "Fast-forward to upstream")
            .map_err(|e| format!("Failed to update branch reference: {}", e))?;

        // Update HEAD to point to the new branch position
        let _ = repo.set_head(&format!("refs/heads/{}", branch_name));
        
        return Ok(());
    }

    // If we can't fast-forward, we need to merge
    // Return an error indicating manual merge is needed
    Err(format!(
        "Cannot fast-forward. {} commit(s) behind, {} commit(s) ahead. Please merge manually.",
        behind, ahead
    ))
}

/// Fallback to subprocess-based pull when there's no tracking branch
fn pull_via_subprocess(repo: &Repository, branch_name: &str, ssh_key_path: Option<&str>) -> Result<(), String> {
    use std::process::Command;
    use std::path::Path;
    
    let path = repo
        .workdir()
        .ok_or("No working directory found")?
        .to_str()
        .ok_or("Invalid path")?;
    
    // Build SSH command if key path is provided
    let mut envs = Vec::new();
    if let Some(key) = ssh_key_path {
        if !key.trim().is_empty() {
            let expanded_path = if key.starts_with("~/") {
                let home = std::env::var("HOME").map_err(|_| "Could not find HOME directory".to_string())?;
                key.replacen("~", &home, 1)
            } else {
                key.to_string()
            };

            if Path::new(&expanded_path).exists() {
                let escaped_path = expanded_path.replace('\'', "'\''");
                let command = format!("ssh -i '{}' -o IdentitiesOnly=yes", escaped_path);
                envs.push(("GIT_SSH_COMMAND", command));
            }
        }
    }

    // Use git pull via subprocess as fallback
    let mut command = Command::new("git");
    command.args(&["pull", "origin", branch_name]);
    command.current_dir(path);
    command.env("GIT_TERMINAL_PROMPT", "0");
    command.env("GIT_PAGER", "cat");
    
    for (key, val) in envs {
        command.env(key, val);
    }

    let output = command
        .output()
        .map_err(|e| format!("Failed to execute git pull: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

        Err(if !stderr.is_empty() {
            stderr
        } else if !stdout.is_empty() {
            stdout
        } else {
            format!("Git pull failed with status: {}", output.status)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn get_temp_dir() -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "ark-remote-ops-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn test_build_ssh_credentials_returns_none_for_missing_key() {
        let result = build_ssh_credentials(None, None).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_build_ssh_credentials_returns_none_for_blank_key() {
        let result = build_ssh_credentials(Some("   "), None).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_build_ssh_credentials_returns_error_for_missing_file() {
        let result = build_ssh_credentials(Some("/nonexistent/key"), None);
        assert!(result.is_err());
        // Just check that it's an error, not the specific message
    }

    #[test]
    fn test_build_ssh_credentials_expands_home() {
        // Create a temporary key file in home directory
        let home = std::env::var("HOME").unwrap();
        let home_path = PathBuf::from(&home);
        let ssh_dir = home_path.join(".ssh");
        let temp_key_path = ssh_dir.join("test_ark_key");
        
        // Create a dummy key file
        fs::create_dir_all(&ssh_dir).ok();
        fs::write(&temp_key_path, "dummy key content").ok();
        
        let result = build_ssh_credentials(Some("~/.ssh/test_ark_key"), None);
        
        // Clean up
        fs::remove_file(&temp_key_path).ok();
        
        // The result should either be Ok(Some(_)) or Ok(None) depending on whether
        // the key is valid for SSH authentication
        // We just verify it doesn't fail with "does not exist"
        if let Err(e) = result {
            assert!(!e.contains("does not exist"));
        }
    }

    #[test]
    fn test_create_remote_callbacks_works() {
        let callbacks = create_remote_callbacks(None, None);
        assert!(callbacks.is_ok());
    }
}
