mod credential_store;
mod git_operations;
mod models;

use models::{
    BranchInfo, BranchOptions, CloneOptions, CommitInfo, CommitOptions, ConflictInfo, DiffInfo,
    FileStatus, RemoteInfo, RepositoryInfo, Settings, SettingsPayload, StageResult, StashInfo, StashOptions,
    TagInfo, TagOptions,
};
use notify::{Config, RecursiveMode, Watcher};
use serde::Deserialize;
use std::path::Path;
use std::sync::Mutex;
use tauri::{Emitter, Listener, Manager, State};
use tokio::time::{timeout, Duration as TokioDuration};

use crate::credential_store::CredentialStore;
use std::sync::atomic::{AtomicBool, Ordering};

static REPO_OP_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

struct RepoOpGuard;

impl RepoOpGuard {
    fn try_acquire() -> Result<Self, AppError> {
        if REPO_OP_IN_PROGRESS.swap(true, Ordering::SeqCst) {
            return Err(AppError::Git(
                "Another repo operation (push/pull/fetch/clone) is already in progress.".to_string()
            ));
        }
        Ok(RepoOpGuard)
    }
}

impl Drop for RepoOpGuard {
    fn drop(&mut self) {
        REPO_OP_IN_PROGRESS.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug)]
pub enum AppError {
    Git(String),
    Io(String),
    Lock(String),
    Config(String),
}

impl serde::Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let msg = match self {
            AppError::Git(e) => format!("Git Error: {}", e),
            AppError::Io(e) => format!("IO Error: {}", e),
            AppError::Lock(e) => format!("Concurrency Error: {}", e),
            AppError::Config(e) => format!("Config Error: {}", e),
        };
        serializer.serialize_str(&msg)
    }
}

impl From<git2::Error> for AppError {
    fn from(err: git2::Error) -> Self {
        AppError::Git(err.to_string())
    }
}

impl From<String> for AppError {
    fn from(err: String) -> Self {
        AppError::Io(err)
    }
}

impl From<&str> for AppError {
    fn from(err: &str) -> Self {
        AppError::Git(err.to_string())
    }
}

struct RepoState {
    repo: Option<git2::Repository>,
    watcher: Option<notify::RecommendedWatcher>,
}

struct SettingsState {
    settings: Settings,
}

struct App {
    repo: Mutex<RepoState>,
    settings: Mutex<SettingsState>,
}

type AppResult<T> = Result<T, AppError>;

fn require_open_repository(repo: Option<&git2::Repository>) -> AppResult<&git2::Repository> {
    repo.ok_or(AppError::Git("No repository open".to_string()))
}

// ponytail: normal .git dir; worktree file-pointer not handled (libgit2 handles internally)
fn resolve_git_dir(repo_path: &str) -> Option<std::path::PathBuf> {
    let git_path = std::path::Path::new(repo_path).join(".git");
    git_path.is_dir().then_some(git_path)
}

fn stop_watcher(watcher: Option<notify::RecommendedWatcher>, repo_path: &str) {
    if let Some(mut w) = watcher {
        let Some(git_path) = resolve_git_dir(repo_path) else { return };
        let _ = w.unwatch(&git_path.join("index"));
        let _ = w.unwatch(&git_path.join("HEAD"));
        let _ = w.unwatch(&git_path.join("refs"));
    }
}

fn start_watcher(app_handle: tauri::AppHandle, repo_path: &str) -> Option<notify::RecommendedWatcher> {
    let git_path = resolve_git_dir(repo_path)?;

    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher = notify::RecommendedWatcher::new(tx, Config::default()).ok()?;

    // Watch key git files for state changes
    let _ = watcher.watch(&git_path.join("index"), RecursiveMode::NonRecursive);
    let _ = watcher.watch(&git_path.join("HEAD"), RecursiveMode::NonRecursive);
    let _ = watcher.watch(&git_path.join("refs"), RecursiveMode::Recursive);

    std::thread::spawn(move || {
        let mut last_emit = std::time::Instant::now();
        let debounce_duration = std::time::Duration::from_millis(300);
        
        while let Ok(res) = rx.recv() {
            match res {
                Ok(_) => {
                    let now = std::time::Instant::now();
                    // Only emit if enough time has passed since last event
                    if now.duration_since(last_emit) >= debounce_duration {
                        last_emit = now;
                        let _ = app_handle.emit("git-state-changed", ());
                    }
                    // Drain the channel of immediate subsequent events
                    while rx.try_recv().is_ok() {}
                }
                Err(e) => tracing::error!("watcher error: {:?}", e),
            }
        }
    });

    Some(watcher)
}

fn get_settings_path(app_handle: &tauri::AppHandle) -> AppResult<std::path::PathBuf> {
    let path = app_handle
        .path()
        .app_config_dir()
        .map_err(|e| AppError::Config(e.to_string()))?;
    if !path.exists() {
        std::fs::create_dir_all(&path).map_err(|e| AppError::Io(e.to_string()))?;
    }
    Ok(path.join("settings.json"))
}

fn default_settings() -> Settings {
    Settings {
        user_name: String::new(),
        user_email: String::new(),
        ssh_key_path: None,
        theme: "dark".to_string(),
        recent_repositories: Vec::new(),
        last_opened_repository: None,
    }
}

fn settings_payload_from_settings(settings: Settings) -> AppResult<SettingsPayload> {
    let ssh_passphrase = settings
        .ssh_key_path
        .as_deref()
        .map(CredentialStore::get_passphrase)
        .transpose()
        .map_err(AppError::Config)?
        .flatten();

    Ok(SettingsPayload {
        settings,
        ssh_passphrase,
    })
}

fn persist_passphrase(
    old_key_path: Option<&str>,
    new_key_path: Option<&str>,
    passphrase: Option<&str>,
) -> AppResult<()> {
    if old_key_path != new_key_path {
        if let Some(old_key_path) = old_key_path {
            CredentialStore::delete_passphrase(old_key_path).map_err(AppError::Config)?;
        }
    }

    match (new_key_path, passphrase) {
        (Some(key_path), Some(passphrase)) if !passphrase.is_empty() => {
            CredentialStore::set_passphrase(key_path, passphrase).map_err(AppError::Config)?;
        }
        (Some(key_path), _) => {
            CredentialStore::delete_passphrase(key_path).map_err(AppError::Config)?;
        }
        (None, Some(passphrase)) if !passphrase.is_empty() => {
            return Err(AppError::Config(
                "SSH key path is required to store passphrase".to_string(),
            ));
        }
        (None, _) => {}
    }

    Ok(())
}

fn save_settings_payload_to_path(payload: &SettingsPayload, path: &Path) -> AppResult<()> {
    persist_passphrase(
        None,
        payload.settings.ssh_key_path.as_deref(),
        payload.ssh_passphrase.as_deref(),
    )?;
    let json = serde_json::to_string_pretty(&payload.settings)
        .map_err(|e| AppError::Config(e.to_string()))?;
    std::fs::write(path, json).map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}

fn load_settings_from_path(path: &Path) -> AppResult<Settings> {
    if !path.exists() {
        return Ok(default_settings());
    }
    let content = std::fs::read_to_string(path).map_err(|e| AppError::Io(e.to_string()))?;

    // ponytail: check for legacy ssh_passphrase before plain Settings parse
    let value: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return Ok(default_settings()),
    };
    if let Some(ref obj) = value.as_object() {
        if obj.contains_key("ssh_passphrase") {
            #[derive(Deserialize)]
            struct Legacy { ssh_passphrase: Option<String>, user_name: String, user_email: String,
                ssh_key_path: Option<String>, theme: String, recent_repositories: Vec<String>,
                last_opened_repository: Option<String> }
            if let Ok(legacy) = serde_json::from_value::<Legacy>(value.clone()) {
                let key_path = legacy.ssh_key_path.clone();
                if let (Some(ref key), Some(ref pass)) = (key_path, legacy.ssh_passphrase) {
                    if !pass.is_empty() {
                        let _ = CredentialStore::set_passphrase(key, &pass);
                    }
                }
                let settings = Settings {
                    user_name: legacy.user_name, user_email: legacy.user_email,
                    ssh_key_path: legacy.ssh_key_path, theme: legacy.theme,
                    recent_repositories: legacy.recent_repositories,
                    last_opened_repository: legacy.last_opened_repository,
                };
                if let Ok(json) = serde_json::to_string_pretty(&settings) {
                    let _ = std::fs::write(path, json);
                }
                return Ok(settings);
            }
        }
    }
    serde_json::from_value::<Settings>(value).or_else(|_| Ok(default_settings()))
}

fn get_ssh_credentials(settings: &Settings) -> AppResult<(Option<String>, Option<String>)> {
    let ssh_key = settings.ssh_key_path.clone();
    let ssh_passphrase = match ssh_key.as_deref() {
        Some(key_path) => CredentialStore::get_passphrase(key_path).map_err(AppError::Config)?,
        None => None,
    };

    Ok((ssh_key, ssh_passphrase))
}

fn save_settings_to_disk(settings: &Settings, app_handle: &tauri::AppHandle) -> AppResult<()> {
    let path = get_settings_path(app_handle)?;
    save_settings_payload_to_path(
        &SettingsPayload {
            settings: settings.clone(),
            ssh_passphrase: settings
                .ssh_key_path
                .as_deref()
                .map(CredentialStore::get_passphrase)
                .transpose()
                .map_err(AppError::Config)?
                .flatten(),
        },
        &path,
    )
}

fn load_settings_from_disk(app_handle: &tauri::AppHandle) -> Settings {
    if let Ok(path) = get_settings_path(app_handle) {
        if let Ok(settings) = load_settings_from_path(&path) {
            return settings;
        }
    }
    default_settings()
}

#[tauri::command]
fn open_repository(
    state: State<'_, App>,
    app_handle: tauri::AppHandle,
    path: String,
) -> AppResult<RepositoryInfo> {
    // Don't switch repos while a remote operation is in flight
    if REPO_OP_IN_PROGRESS.load(Ordering::SeqCst) {
        return Err(AppError::Git(
            "Cannot switch repositories while a push/pull/fetch/clone is in progress. Wait for it to complete.".to_string()
        ));
    }
    let mut repo_state = state.repo.lock().unwrap();
    let mut settings_state = state.settings.lock().unwrap();
    match git_operations::open_repository(&path) {
        Ok(repo) => {
            let info = git_operations::get_repository_info(&repo)?;
            repo_state.repo = Some(repo);
            stop_watcher(repo_state.watcher.take(), &path);
            repo_state.watcher = start_watcher(app_handle.clone(), &path);

            // Clean up old safety refs (older than 7 days)
            if let Ok(repo_ref) = git_operations::open_repository(&path) {
                let _ = git_operations::cleanup_safety_refs(&repo_ref, 7 * 24 * 3600);
            }

            // Add to recent repositories if not already there
            if !settings_state.settings.recent_repositories.contains(&path) {
                settings_state.settings.recent_repositories.insert(0, path.clone());
                if settings_state.settings.recent_repositories.len() > 10 {
                    settings_state.settings.recent_repositories.truncate(10);
                }
            }
            settings_state.settings.last_opened_repository = Some(path);
            save_settings_to_disk(&settings_state.settings, &app_handle)?;
            Ok(info)
        }
        Err(e) => {
            if !std::path::Path::new(&path).exists() {
                settings_state.settings.recent_repositories.retain(|p| p != &path);
                if settings_state.settings.last_opened_repository == Some(path) {
                    settings_state.settings.last_opened_repository = None;
                }
                let _ = save_settings_to_disk(&settings_state.settings, &app_handle);
                return Err(AppError::Git("Repository path not found. Removed from list.".to_string()));
            }
            Err(AppError::Git(e))
        }
    }
}

#[tauri::command]
async fn clone_repository(
    state: State<'_, App>,
    app_handle: tauri::AppHandle,
    options: CloneOptions,
) -> AppResult<String> {
    let _guard = RepoOpGuard::try_acquire()?;

    let (ssh_key, ssh_pass) = {
        let settings_state = state.settings.lock().unwrap();
        get_ssh_credentials(&settings_state.settings)?
    };

    let url = options.url.clone();
    let path = options.path.clone();

    let repo_path = path.clone();
    timeout(TokioDuration::from_secs(300), async {
        tauri::async_runtime::spawn_blocking(move || {
            git_operations::clone_repository(
                &url,
                &repo_path,
                ssh_key.as_deref(),
                ssh_pass.as_deref(),
            )
        })
        .await
        .map_err(|e| AppError::Git(format!("Spawn error: {}", e)))?
        .map_err(AppError::Git)
    })
    .await
    .map_err(|_| AppError::Git("Clone timed out after 300 seconds".to_string()))??;

    let mut repo_state = state.repo.lock().unwrap();
    let mut settings_state = state.settings.lock().unwrap();
    // Re-open repo in state
    match git_operations::open_repository(&path) {
        Ok(repo) => {
            repo_state.repo = Some(repo);
            stop_watcher(repo_state.watcher.take(), &path);
            repo_state.watcher = start_watcher(app_handle.clone(), &path);

            if !settings_state.settings.recent_repositories.contains(&path) {
                settings_state.settings.recent_repositories.insert(0, path.clone());
            }
            settings_state.settings.last_opened_repository = Some(path.clone());
            save_settings_to_disk(&settings_state.settings, &app_handle)?;
            Ok(path)
        }
        Err(e) => Err(AppError::Git(e)),
    }
}

#[tauri::command]
fn get_repository_status(state: State<'_, App>) -> AppResult<Vec<FileStatus>> {
    let state = state.repo.lock().unwrap();
    let repo = require_open_repository(state.repo.as_ref())?;
    git_operations::get_status(repo).map_err(AppError::Git)
}

#[tauri::command]
fn create_commit(state: State<'_, App>, options: CommitOptions) -> AppResult<String> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    let stage_result = git_operations::stage_files(repo, options.files)?;
    if stage_result.staged.is_empty() && !stage_result.warnings.is_empty() {
        return Err(AppError::Git(format!("No files could be staged: {}", stage_result.warnings.join("; "))));
    }
    git_operations::create_commit(repo, &options.message).map_err(AppError::Git)
}

#[tauri::command]
fn stage_files(state: State<'_, App>, files: Vec<String>) -> AppResult<StageResult> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::stage_files(repo, files).map_err(AppError::Git)
}

#[tauri::command]
fn unstage_files(state: State<'_, App>, files: Vec<String>) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::unstage_files(repo, files).map_err(AppError::Git)
}

#[tauri::command]
fn discard_changes(state: State<'_, App>, file_path: String) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::discard_changes(repo, &file_path).map_err(AppError::Git)
}

#[tauri::command]
fn get_branches(state: State<'_, App>) -> AppResult<Vec<BranchInfo>> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::get_branches(repo).map_err(AppError::Git)
}

#[tauri::command]
fn create_branch(state: State<'_, App>, options: BranchOptions) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::create_branch(repo, &options.name, options.start_sha.as_deref()).map_err(AppError::Git)?;
    Ok(())
}

#[tauri::command]
fn checkout_branch(state: State<'_, App>, options: BranchOptions) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::checkout_branch(repo, &options.name).map_err(AppError::Git)?;
    Ok(())
}

#[tauri::command]
fn get_commit_diff(state: State<'_, App>, sha: String) -> AppResult<Vec<DiffInfo>> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::get_commit_diff(repo, &sha).map_err(AppError::Git)
}

#[tauri::command]
fn get_commit_history(state: State<'_, App>, limit: usize) -> AppResult<Vec<CommitInfo>> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::get_commit_history(repo, limit).map_err(AppError::Git)
}

#[tauri::command]
fn get_diff(state: State<'_, App>, file_path: Option<String>) -> AppResult<Vec<DiffInfo>> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::get_diff(repo, file_path.as_deref()).map_err(AppError::Git)
}

#[tauri::command]
async fn push_changes(state: State<'_, App>) -> AppResult<()> {
    let _guard = RepoOpGuard::try_acquire()?;

    let (path, ssh_key, ssh_pass) = {
        let repo_state = state.repo.lock().unwrap();
        let repo = repo_state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
        let path = repo.workdir().ok_or(AppError::Git("No workdir".to_string()))?.to_path_buf();
        let settings_state = state.settings.lock().unwrap();
        let (ssh_key, ssh_pass) = get_ssh_credentials(&settings_state.settings)?;
        drop(settings_state);
        (path, ssh_key, ssh_pass)
    };

    let result = timeout(TokioDuration::from_secs(120), async {
        tauri::async_runtime::spawn_blocking(move || {
            let repo = git_operations::open_repository(path.to_str().ok_or("Invalid path")?)?;
            git_operations::push_changes(&repo, ssh_key.as_deref(), ssh_pass.as_deref()).map_err(AppError::Git)
        })
        .await
        .map_err(|e| AppError::Git(format!("Spawn error: {}", e)))?
    })
    .await;
    result.map_err(|_| AppError::Git("Push timed out after 120 seconds".to_string()))?
}

#[tauri::command]
async fn pull_changes(state: State<'_, App>) -> AppResult<()> {
    let _guard = RepoOpGuard::try_acquire()?;

    let (path, ssh_key, ssh_pass) = {
        let repo_state = state.repo.lock().unwrap();
        let repo = repo_state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
        let path = repo.workdir().ok_or(AppError::Git("No workdir".to_string()))?.to_path_buf();
        let settings_state = state.settings.lock().unwrap();
        let (ssh_key, ssh_pass) = get_ssh_credentials(&settings_state.settings)?;
        drop(settings_state);
        (path, ssh_key, ssh_pass)
    };

    let result = timeout(TokioDuration::from_secs(120), async {
        tauri::async_runtime::spawn_blocking(move || {
            let repo = git_operations::open_repository(path.to_str().ok_or("Invalid path")?)?;
            git_operations::pull_changes(&repo, ssh_key.as_deref(), ssh_pass.as_deref()).map_err(AppError::Git)
        })
        .await
        .map_err(|e| AppError::Git(format!("Spawn error: {}", e)))?
    })
    .await;
    result.map_err(|_| AppError::Git("Pull timed out after 120 seconds".to_string()))?
}

#[tauri::command]
async fn fetch_changes(state: State<'_, App>) -> AppResult<()> {
    let _guard = RepoOpGuard::try_acquire()?;

    let (path, ssh_key, ssh_pass) = {
        let repo_state = state.repo.lock().unwrap();
        let repo = repo_state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
        let path = repo.workdir().ok_or(AppError::Git("No workdir".to_string()))?.to_path_buf();
        let settings_state = state.settings.lock().unwrap();
        let (ssh_key, ssh_pass) = get_ssh_credentials(&settings_state.settings)?;
        drop(settings_state);
        (path, ssh_key, ssh_pass)
    };

    let result = timeout(TokioDuration::from_secs(120), async {
        tauri::async_runtime::spawn_blocking(move || {
            let repo = git_operations::open_repository(path.to_str().ok_or("Invalid path")?)?;
            git_operations::fetch_changes(&repo, ssh_key.as_deref(), ssh_pass.as_deref()).map_err(AppError::Git)
        })
        .await
        .map_err(|e| AppError::Git(format!("Spawn error: {}", e)))?
    })
    .await;
    result.map_err(|_| AppError::Git("Fetch timed out after 120 seconds".to_string()))?
}

#[tauri::command]
fn stash_save(state: State<'_, App>, options: StashOptions) -> AppResult<()> {
    let mut state = state.repo.lock().unwrap();
    let repo = state.repo.as_mut().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::stash_save(repo, options.message.as_deref()).map_err(AppError::Git)
}

#[tauri::command]
fn stash_pop(state: State<'_, App>, index: usize) -> AppResult<()> {
    let mut state = state.repo.lock().unwrap();
    let repo = state.repo.as_mut().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::stash_pop(repo, index).map_err(AppError::Git)
}

#[tauri::command]
fn list_stashes(state: State<'_, App>) -> AppResult<Vec<StashInfo>> {
    let mut state = state.repo.lock().unwrap();
    let repo = state.repo.as_mut().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::stash_list(repo).map_err(AppError::Git)
}

#[tauri::command]
fn get_conflicts(state: State<'_, App>) -> AppResult<Vec<ConflictInfo>> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::get_conflicts(repo).map_err(AppError::Git)
}

#[tauri::command]
fn resolve_conflict(state: State<'_, App>, path: String, use_ours: bool) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::resolve_conflict(repo, &path, use_ours).map_err(AppError::Git)
}

#[tauri::command]
fn amend_commit(state: State<'_, App>, message: String) -> AppResult<String> {
    let state = state.repo.lock().unwrap();
    let repo = require_open_repository(state.repo.as_ref())?;
    git_operations::amend_last_commit(repo, &message).map_err(AppError::Git)
}

#[tauri::command]
fn cherry_pick(state: State<'_, App>, sha: String) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = require_open_repository(state.repo.as_ref())?;
    git_operations::cherry_pick(repo, &sha).map_err(AppError::Git)
}

#[tauri::command]
fn revert_commit(state: State<'_, App>, sha: String) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = require_open_repository(state.repo.as_ref())?;
    git_operations::revert_commit(repo, &sha).map_err(AppError::Git)
}

#[tauri::command]
fn discard_all_changes(state: State<'_, App>) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::discard_all_changes(repo).map_err(AppError::Git)
}

#[tauri::command]
fn get_settings(state: State<'_, App>) -> AppResult<SettingsPayload> {
    let state = state.settings.lock().unwrap();
    settings_payload_from_settings(state.settings.clone())
}

#[tauri::command]
fn save_settings(
    state: State<'_, App>,
    app_handle: tauri::AppHandle,
    settings: SettingsPayload,
) -> AppResult<()> {
    // Capture old key path before acquiring the settings lock
    let previous_key_path = {
        let state = state.settings.lock().unwrap();
        state.settings.ssh_key_path.clone()
    };
    // persist_passphrase touches keyring — do it WITHOUT holding the settings lock
    persist_passphrase(
        previous_key_path.as_deref(),
        settings.settings.ssh_key_path.as_deref(),
        settings.ssh_passphrase.as_deref(),
    )?;
    // Now update and save settings (short lock)
    let mut state = state.settings.lock().unwrap();
    state.settings = settings.settings;
    save_settings_to_disk(&state.settings, &app_handle)?;
    Ok(())
}

#[tauri::command]
fn set_remote_url(state: State<'_, App>, name: String, url: String) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::set_remote_url(repo, &name, &url).map_err(AppError::Git)
}

#[tauri::command]
fn get_remote_url(state: State<'_, App>, name: String) -> AppResult<String> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::get_remote_url(repo, &name).map_err(AppError::Git)
}

#[tauri::command]
async fn get_repositories_info(
    state: State<'_, App>,
    app_handle: tauri::AppHandle,
    paths: Vec<String>,
) -> AppResult<Vec<RepositoryInfo>> {
    let mut results = Vec::new();
    let mut to_remove = Vec::new();

    for path in paths {
        match git_operations::open_repository(&path) {
            Ok(repo) => {
                if let Ok(info) = git_operations::get_repository_info(&repo) {
                    results.push(info);
                    continue;
                }
            }
            Err(_) => {
                if !std::path::Path::new(&path).exists() {
                    to_remove.push(path.clone());
                    continue;
                }
            }
        }
        // Fallback for valid paths that can't be opened or other errors
        results.push(RepositoryInfo {
            path,
            current_branch: "unknown".to_string(),
            is_dirty: false,
            ahead: 0,
            behind: 0,
        });
    }

    if !to_remove.is_empty() {
        let mut state = state.settings.lock().unwrap();
        state.settings.recent_repositories.retain(|p| !to_remove.contains(p));
        let _ = save_settings_to_disk(&state.settings, &app_handle);
    }

    Ok(results)
}

#[tauri::command]
fn get_current_repo_info(state: State<'_, App>) -> AppResult<Option<RepositoryInfo>> {
    let state = state.repo.lock().unwrap();
    if let Some(repo) = state.repo.as_ref() {
        let info = git_operations::get_repository_info(repo).map_err(AppError::Git)?;
        Ok(Some(info))
    } else {
        Ok(None)
    }
}

#[tauri::command]
fn reveal_in_finder(state: State<'_, App>, path: String) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    let workdir = repo.workdir().ok_or(AppError::Git("No workdir".to_string()))?;
    let full_path = std::path::Path::new(&path);
    let canonical = full_path.canonicalize().map_err(|e| AppError::Io(e.to_string()))?;
    if !canonical.starts_with(workdir.canonicalize().map_err(|e| AppError::Io(e.to_string()))?) {
        return Err(AppError::Git("Path is outside the repository".to_string()));
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| AppError::Io(e.to_string()))?;
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&path)
            .spawn()
            .map_err(|e| AppError::Io(e.to_string()))?;
    }
    #[cfg(target_os = "linux")]
    {
        let p = std::path::Path::new(&path);
        let dir = if p.is_dir() {
            p
        } else {
            p.parent().unwrap_or(p)
        };
        std::process::Command::new("xdg-open")
            .arg(dir)
            .spawn()
            .map_err(|e| AppError::Io(e.to_string()))?;
    }
    Ok(())
}

#[tauri::command]
fn add_to_gitignore(state: State<'_, App>, file_path: String) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::add_to_gitignore(repo, &file_path).map_err(AppError::Git)
}

#[tauri::command]
fn read_file(state: State<'_, App>, file_path: String) -> AppResult<String> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::read_file(repo, &file_path).map_err(AppError::Git)
}

#[tauri::command]
fn create_tag(state: State<'_, App>, options: TagOptions) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::create_tag(repo, &options.name, &options.message, &options.sha).map_err(AppError::Git)
}

#[tauri::command]
fn drop_stash(state: State<'_, App>, index: usize) -> AppResult<()> {
    let mut state = state.repo.lock().unwrap();
    let repo = state.repo.as_mut().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::stash_drop(repo, index).map_err(AppError::Git)
}

#[tauri::command]
fn apply_stash(state: State<'_, App>, index: usize) -> AppResult<()> {
    let mut state = state.repo.lock().unwrap();
    let repo = state.repo.as_mut().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::stash_apply(repo, index).map_err(AppError::Git)
}

#[tauri::command]
fn branch_from_stash(state: State<'_, App>, index: usize, branch_name: String) -> AppResult<()> {
    let mut state = state.repo.lock().unwrap();
    let repo = state.repo.as_mut().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::stash_branch(repo, index, &branch_name).map_err(AppError::Git)
}

#[tauri::command]
fn reset_branch(state: State<'_, App>, sha: String) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::reset_branch(repo, &sha).map_err(AppError::Git)
}

#[tauri::command]
fn merge_commit(state: State<'_, App>, sha: String) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::merge_commit(repo, &sha).map_err(AppError::Git)
}

#[tauri::command]
fn list_tags(state: State<'_, App>) -> AppResult<Vec<TagInfo>> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::list_tags(repo).map_err(AppError::Git)
}

#[tauri::command]
fn delete_tag(state: State<'_, App>, name: String) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::delete_tag(repo, &name).map_err(AppError::Git)
}

#[tauri::command]
fn list_remotes(state: State<'_, App>) -> AppResult<Vec<RemoteInfo>> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::list_remotes(repo).map_err(AppError::Git)
}

#[tauri::command]
fn add_remote(state: State<'_, App>, name: String, url: String) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::add_remote(repo, &name, &url).map_err(AppError::Git)
}

#[tauri::command]
fn remove_remote(state: State<'_, App>, name: String) -> AppResult<()> {
    let state = state.repo.lock().unwrap();
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::remove_remote(repo, &name).map_err(AppError::Git)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle();
            let settings = load_settings_from_disk(app_handle);
            let mut repo = None;
            let mut watcher = None;
            if let Some(path) = &settings.last_opened_repository {
                if let Ok(opened_repo) = git_operations::open_repository(path) {
                    repo = Some(opened_repo);
                    watcher = start_watcher(app_handle.clone(), path);
                }
            }
            app.manage(App {
                repo: Mutex::new(RepoState {
                    repo,
                    watcher,
                }),
                settings: Mutex::new(SettingsState { settings }),
            });

            // Cleanup on app exit
            let app_handle_clone = app_handle.clone();
            app.listen("tauri://close-requested", move |_| {
                if let Some(state) = app_handle_clone.try_state::<App>() {
                    if let Ok(mut repo_state) = state.repo.lock() {
                        let repo_path = repo_state.repo.as_ref()
                            .and_then(|r| r.workdir())
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_default();
                        stop_watcher(repo_state.watcher.take(), &repo_path);
                    }
                }
            });

            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_repository,
            clone_repository,
            get_repository_status,
            create_commit,
            amend_commit,
            cherry_pick,
            revert_commit,
            discard_all_changes,
            stage_files,
            unstage_files,
            discard_changes,
            get_branches,
            create_branch,
            checkout_branch,
            get_commit_diff,
            get_commit_history,
            get_diff,
            push_changes,
            pull_changes,
            fetch_changes,
            stash_save,
            stash_pop,
            list_stashes,
            get_conflicts,
            resolve_conflict,
            get_settings,
            save_settings,
            set_remote_url,
            get_remote_url,
            get_current_repo_info,
            get_repositories_info,
            reveal_in_finder,
            add_to_gitignore,
            read_file,
            create_tag,
            drop_stash,
            apply_stash,
            branch_from_stash,
            reset_branch,
            merge_commit,
            list_tags,
            delete_tag,
            list_remotes,
            add_remote,
            remove_remote,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn get_temp_dir() -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "ark-settings-test-{}-{}",
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
    fn test_require_open_repository_returns_no_repository_open_error() {
        let err = match require_open_repository(None) {
            Ok(_) => panic!("require_open_repository unexpectedly succeeded"),
            Err(err) => err,
        };
        let serialized = serde_json::to_string(&err).unwrap();

        assert_eq!(serialized, "\"Git Error: No repository open\"");
    }

    #[test]
    fn test_save_settings_to_path_omits_passphrase_from_json_and_stores_keyring_secret() {
        crate::credential_store::CredentialStore::clear_all();

        let root = get_temp_dir();
        let settings_path = root.join("settings.json");
        let key_path = root.join("id_ed25519");

        let settings = models::SettingsPayload {
            settings: Settings {
                user_name: "Ark User".to_string(),
                user_email: "ark@example.com".to_string(),
                ssh_key_path: Some(key_path.to_string_lossy().to_string()),
                theme: "dark".to_string(),
                recent_repositories: vec!["/tmp/repo".to_string()],
                last_opened_repository: Some("/tmp/repo".to_string()),
            },
            ssh_passphrase: Some("super-secret".to_string()),
        };

        save_settings_payload_to_path(&settings, &settings_path).unwrap();

        let disk_json = fs::read_to_string(&settings_path).unwrap();
        assert!(!disk_json.contains("ssh_passphrase"));
        assert!(!disk_json.contains("super-secret"));

        let stored = crate::credential_store::CredentialStore::get_passphrase(
            key_path.to_string_lossy().as_ref(),
        )
        .unwrap();
        assert_eq!(stored.as_deref(), Some("super-secret"));
    }

    #[test]
    fn test_load_settings_from_path_migrates_legacy_plaintext_passphrase_to_keyring() {
        crate::credential_store::CredentialStore::clear_all();

        let root = get_temp_dir();
        let settings_path = root.join("settings.json");
        let key_path = root.join("id_rsa");

        fs::write(
            &settings_path,
            serde_json::json!({
                "user_name": "Legacy User",
                "user_email": "legacy@example.com",
                "ssh_key_path": key_path.to_string_lossy().to_string(),
                "ssh_passphrase": "legacy-secret",
                "theme": "dark",
                "recent_repositories": ["/tmp/legacy"],
                "last_opened_repository": "/tmp/legacy"
            })
            .to_string(),
        )
        .unwrap();

        let loaded = load_settings_from_path(&settings_path).unwrap();

        assert_eq!(loaded.user_name, "Legacy User");
        assert_eq!(loaded.ssh_key_path.as_deref(), Some(key_path.to_string_lossy().as_ref()));

        let stored = crate::credential_store::CredentialStore::get_passphrase(
            key_path.to_string_lossy().as_ref(),
        )
        .unwrap();
        assert_eq!(stored.as_deref(), Some("legacy-secret"));

        let migrated_json = fs::read_to_string(&settings_path).unwrap();
        assert!(!migrated_json.contains("ssh_passphrase"));
        assert!(!migrated_json.contains("legacy-secret"));
    }

}
