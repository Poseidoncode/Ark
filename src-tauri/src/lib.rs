mod credential_store;
mod git_operations;
mod models;

use models::{
    BranchInfo, BranchOptions, CloneOptions, CommitInfo, CommitOptions, ConflictInfo, DiffInfo,
    FileStatus, RepositoryInfo, Settings, SettingsPayload, StageResult, StashInfo, StashOptions,
};
use notify::{Config, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use tauri::{Emitter, Listener, Manager, State};

use crate::credential_store::CredentialStore;

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
        AppError::Git(err)
    }
}

impl From<&str> for AppError {
    fn from(err: &str) -> Self {
        AppError::Git(err.to_string())
    }
}

struct AppState {
    repo: Option<git2::Repository>,
    settings: Settings,
    watcher: Option<notify::RecommendedWatcher>,
}

struct App(Mutex<AppState>);

type AppResult<T> = Result<T, AppError>;

fn require_open_repository(repo: Option<&git2::Repository>) -> AppResult<&git2::Repository> {
    repo.ok_or(AppError::Git("No repository open".to_string()))
}

fn stop_watcher(watcher: Option<notify::RecommendedWatcher>) {
    if let Some(mut w) = watcher {
        let _ = w.unwatch(&std::path::PathBuf::from(".git/index"));
        let _ = w.unwatch(&std::path::PathBuf::from(".git/HEAD"));
        let _ = w.unwatch(&std::path::PathBuf::from(".git/refs"));
    }
}

fn start_watcher(app_handle: tauri::AppHandle, repo_path: &str) -> Option<notify::RecommendedWatcher> {
    let path = std::path::Path::new(repo_path);
    let git_path = path.join(".git");

    if !git_path.exists() {
        return None;
    }

    let (tx, rx) = std::sync::mpsc::channel();

    let mut watcher = notify::RecommendedWatcher::new(tx, Config::default()).ok()?;

    // Watch key git files for state changes
    let _ = watcher.watch(&git_path.join("index"), RecursiveMode::NonRecursive);
    let _ = watcher.watch(&git_path.join("HEAD"), RecursiveMode::NonRecursive);
    let _ = watcher.watch(&git_path.join("refs"), RecursiveMode::Recursive);

    std::thread::spawn(move || {
        // Simple debounce: wait a bit and clear the channel of rapid events
        while let Ok(res) = rx.recv() {
            match res {
                Ok(_) => {
                    // Give Git a moment to finish its IO
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    let _ = app_handle.emit("git-state-changed", ());

                    // Drain the channel of immediate subsequent events
                    while let Ok(_) = rx.try_recv() {}
                }
                Err(e) => eprintln!("watcher error: {:?}", e),
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

#[derive(Debug, Serialize, Deserialize)]
struct DiskSettings {
    user_name: String,
    user_email: String,
    ssh_key_path: Option<String>,
    theme: String,
    recent_repositories: Vec<String>,
    last_opened_repository: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LegacyDiskSettings {
    user_name: String,
    user_email: String,
    ssh_key_path: Option<String>,
    ssh_passphrase: Option<String>,
    theme: String,
    recent_repositories: Vec<String>,
    last_opened_repository: Option<String>,
}

impl From<Settings> for DiskSettings {
    fn from(settings: Settings) -> Self {
        Self {
            user_name: settings.user_name,
            user_email: settings.user_email,
            ssh_key_path: settings.ssh_key_path,
            theme: settings.theme,
            recent_repositories: settings.recent_repositories,
            last_opened_repository: settings.last_opened_repository,
        }
    }
}

impl From<DiskSettings> for Settings {
    fn from(settings: DiskSettings) -> Self {
        Self {
            user_name: settings.user_name,
            user_email: settings.user_email,
            ssh_key_path: settings.ssh_key_path,
            theme: settings.theme,
            recent_repositories: settings.recent_repositories,
            last_opened_repository: settings.last_opened_repository,
        }
    }
}

impl From<LegacyDiskSettings> for Settings {
    fn from(settings: LegacyDiskSettings) -> Self {
        Self {
            user_name: settings.user_name,
            user_email: settings.user_email,
            ssh_key_path: settings.ssh_key_path,
            theme: settings.theme,
            recent_repositories: settings.recent_repositories,
            last_opened_repository: settings.last_opened_repository,
        }
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

    let json = serde_json::to_string_pretty(&DiskSettings::from(payload.settings.clone()))
        .map_err(|e| AppError::Config(e.to_string()))?;
    std::fs::write(path, json).map_err(|e| AppError::Io(e.to_string()))?;
    Ok(())
}

fn migrate_legacy_passphrase(path: &Path, legacy: LegacyDiskSettings) -> AppResult<Settings> {
    if let (Some(key_path), Some(passphrase)) = (
        legacy.ssh_key_path.as_deref(),
        legacy.ssh_passphrase.as_deref(),
    ) {
        if !passphrase.is_empty() {
            CredentialStore::set_passphrase(key_path, passphrase).map_err(AppError::Config)?;
        }
    }

    let settings = Settings::from(legacy);
    let json = serde_json::to_string_pretty(&DiskSettings::from(settings.clone()))
        .map_err(|e| AppError::Config(e.to_string()))?;
    std::fs::write(path, json).map_err(|e| AppError::Io(e.to_string()))?;
    Ok(settings)
}

fn load_settings_from_path(path: &Path) -> AppResult<Settings> {
    if !path.exists() {
        return Ok(default_settings());
    }

    let content = std::fs::read_to_string(path).map_err(|e| AppError::Io(e.to_string()))?;

    if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
        if value.get("ssh_passphrase").is_some() {
            let legacy = serde_json::from_value::<LegacyDiskSettings>(value)
                .map_err(|e| AppError::Config(e.to_string()))?;
            return migrate_legacy_passphrase(path, legacy);
        }
    }

    if let Ok(settings) = serde_json::from_str::<DiskSettings>(&content) {
        return Ok(settings.into());
    }

    if let Ok(legacy) = serde_json::from_str::<LegacyDiskSettings>(&content) {
        return migrate_legacy_passphrase(path, legacy);
    }

    Ok(default_settings())
}

fn get_ssh_credentials(settings: &Settings) -> AppResult<(Option<String>, Option<String>)> {
    let ssh_key = settings.ssh_key_path.clone();
    let ssh_passphrase = match ssh_key.as_deref() {
        Some(key_path) => CredentialStore::get_passphrase(key_path).map_err(AppError::Config)?,
        None => None,
    };

    Ok((ssh_key, ssh_passphrase))
}

fn save_settings_to_disk(state: &AppState, app_handle: &tauri::AppHandle) -> AppResult<()> {
    let path = get_settings_path(app_handle)?;
    save_settings_payload_to_path(
        &SettingsPayload {
            settings: state.settings.clone(),
            ssh_passphrase: state
                .settings
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
    let mut state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    match git_operations::open_repository(&path) {
        Ok(repo) => {
            let info = git_operations::get_repository_info(&repo)?;
            state.repo = Some(repo);
            stop_watcher(state.watcher.take());
            state.watcher = start_watcher(app_handle.clone(), &path);

            // Add to recent repositories if not already there
            if !state.settings.recent_repositories.contains(&path) {
                state.settings.recent_repositories.insert(0, path.clone());
                if state.settings.recent_repositories.len() > 10 {
                    state.settings.recent_repositories.truncate(10);
                }
            }
            state.settings.last_opened_repository = Some(path);
            save_settings_to_disk(&state, &app_handle)?;
            Ok(info)
        }
        Err(e) => {
            if !std::path::Path::new(&path).exists() {
                state.settings.recent_repositories.retain(|p| p != &path);
                if state.settings.last_opened_repository == Some(path) {
                    state.settings.last_opened_repository = None;
                }
                let _ = save_settings_to_disk(&state, &app_handle);
                return Err(AppError::Git(format!("Repository path not found. Removed from list.")));
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
    let (ssh_key, ssh_pass) = {
        let state_lock = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
        get_ssh_credentials(&state_lock.settings)?
    };

    let url = options.url.clone();
    let path = options.path.clone();

    // Perform clone in a blocking thread to avoid freezing the async executor
    let repo_path = path.clone();
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
    .map_err(AppError::Git)?;

    // Re-acquire lock to update state
    let mut state_lock = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    
    // Re-open repo in state
    match git_operations::open_repository(&path) {
        Ok(repo) => {
            state_lock.repo = Some(repo);
            stop_watcher(state_lock.watcher.take());
            state_lock.watcher = start_watcher(app_handle.clone(), &path);

            if !state_lock.settings.recent_repositories.contains(&path) {
                state_lock.settings.recent_repositories.insert(0, path.clone());
            }
            state_lock.settings.last_opened_repository = Some(path.clone());
            save_settings_to_disk(&state_lock, &app_handle)?;
            Ok(path)
        }
        Err(e) => Err(AppError::Git(e)),
    }
}

#[tauri::command]
fn get_repository_status(state: State<'_, App>) -> AppResult<Vec<FileStatus>> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = require_open_repository(state.repo.as_ref())?;
    git_operations::get_status(repo).map_err(AppError::Git)
}

#[tauri::command]
fn create_commit(state: State<'_, App>, options: CommitOptions) -> AppResult<String> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    let stage_result = git_operations::stage_files(repo, options.files)?;
    if stage_result.staged.is_empty() && !stage_result.warnings.is_empty() {
        return Err(AppError::Git(format!("No files could be staged: {}", stage_result.warnings.join("; "))));
    }
    git_operations::create_commit(repo, &options.message).map_err(AppError::Git)
}

#[tauri::command]
fn stage_files(state: State<'_, App>, files: Vec<String>) -> AppResult<StageResult> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::stage_files(repo, files).map_err(AppError::Git)
}

#[tauri::command]
fn unstage_files(state: State<'_, App>, files: Vec<String>) -> AppResult<()> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::unstage_files(repo, files).map_err(AppError::Git)
}

#[tauri::command]
fn discard_changes(state: State<'_, App>, file_path: String) -> AppResult<()> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::discard_changes(repo, &file_path).map_err(AppError::Git)
}

#[tauri::command]
fn get_branches(state: State<'_, App>) -> AppResult<Vec<BranchInfo>> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::get_branches(repo).map_err(AppError::Git)
}

#[tauri::command]
fn create_branch(state: State<'_, App>, options: BranchOptions) -> AppResult<()> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::create_branch(repo, &options.name, options.start_sha.as_deref()).map_err(AppError::Git)
}

#[tauri::command]
fn checkout_branch(state: State<'_, App>, options: BranchOptions) -> AppResult<()> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::checkout_branch(repo, &options.name).map_err(AppError::Git)
}

#[tauri::command]
fn get_commit_diff(state: State<'_, App>, sha: String) -> AppResult<Vec<DiffInfo>> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::get_commit_diff(repo, &sha).map_err(AppError::Git)
}

#[tauri::command]
fn get_commit_history(state: State<'_, App>, limit: usize) -> AppResult<Vec<CommitInfo>> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::get_commit_history(repo, limit).map_err(AppError::Git)
}

#[tauri::command]
fn get_diff(state: State<'_, App>, file_path: Option<String>) -> AppResult<Vec<DiffInfo>> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::get_diff(repo, file_path.as_deref()).map_err(AppError::Git)
}

#[tauri::command]
async fn push_changes(state: State<'_, App>) -> AppResult<()> {
    let (path, ssh_key, ssh_pass) = {
        let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
        let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
        let path = repo.workdir().ok_or(AppError::Git("No workdir".to_string()))?.to_path_buf();
        let (ssh_key, ssh_pass) = get_ssh_credentials(&state.settings)?;
        (path, ssh_key, ssh_pass)
    };

    tauri::async_runtime::spawn_blocking(move || {
        let repo = git_operations::open_repository(path.to_str().ok_or("Invalid path")?)?;
        git_operations::push_changes(
            &repo,
            ssh_key.as_deref(),
            ssh_pass.as_deref(),
        ).map_err(AppError::Git)
    })
    .await
    .map_err(|e| AppError::Git(format!("Spawn error: {}", e)))?
}

#[tauri::command]
async fn pull_changes(state: State<'_, App>) -> AppResult<()> {
    let (path, ssh_key, ssh_pass) = {
        let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
        let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
        let path = repo.workdir().ok_or(AppError::Git("No workdir".to_string()))?.to_path_buf();
        let (ssh_key, ssh_pass) = get_ssh_credentials(&state.settings)?;
        (path, ssh_key, ssh_pass)
    };

    tauri::async_runtime::spawn_blocking(move || {
        let repo = git_operations::open_repository(path.to_str().ok_or("Invalid path")?)?;
        git_operations::pull_changes(
            &repo,
            ssh_key.as_deref(),
            ssh_pass.as_deref(),
        ).map_err(AppError::Git)
    })
    .await
    .map_err(|e| AppError::Git(format!("Spawn error: {}", e)))?
}

#[tauri::command]
async fn fetch_changes(state: State<'_, App>) -> AppResult<()> {
    let (path, ssh_key, ssh_pass) = {
        let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
        let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
        let path = repo.workdir().ok_or(AppError::Git("No workdir".to_string()))?.to_path_buf();
        let (ssh_key, ssh_pass) = get_ssh_credentials(&state.settings)?;
        (path, ssh_key, ssh_pass)
    };

    tauri::async_runtime::spawn_blocking(move || {
        let repo = git_operations::open_repository(path.to_str().ok_or("Invalid path")?)?;
        git_operations::fetch_changes(
            &repo,
            ssh_key.as_deref(),
            ssh_pass.as_deref(),
        ).map_err(AppError::Git)
    })
    .await
    .map_err(|e| AppError::Git(format!("Spawn error: {}", e)))?
}

#[tauri::command]
fn stash_save(state: State<'_, App>, options: StashOptions) -> AppResult<()> {
    let mut state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_mut().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::stash_save(repo, options.message.as_deref()).map_err(AppError::Git)
}

#[tauri::command]
fn stash_pop(state: State<'_, App>, index: usize) -> AppResult<()> {
    let mut state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_mut().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::stash_pop(repo, index).map_err(AppError::Git)
}

#[tauri::command]
fn list_stashes(state: State<'_, App>) -> AppResult<Vec<StashInfo>> {
    let mut state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_mut().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::stash_list(repo).map_err(AppError::Git)
}

#[tauri::command]
fn get_conflicts(state: State<'_, App>) -> AppResult<Vec<ConflictInfo>> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::get_conflicts(repo).map_err(AppError::Git)
}

#[tauri::command]
fn resolve_conflict(state: State<'_, App>, path: String, use_ours: bool) -> AppResult<()> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::resolve_conflict(repo, &path, use_ours).map_err(AppError::Git)
}

#[tauri::command]
fn amend_commit(state: State<'_, App>, message: String) -> AppResult<String> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = require_open_repository(state.repo.as_ref())?;
    git_operations::amend_last_commit(repo, &message).map_err(AppError::Git)
}

#[tauri::command]
fn cherry_pick(state: State<'_, App>, sha: String) -> AppResult<()> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = require_open_repository(state.repo.as_ref())?;
    git_operations::cherry_pick(repo, &sha).map_err(AppError::Git)
}

#[tauri::command]
fn revert_commit(state: State<'_, App>, sha: String) -> AppResult<()> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = require_open_repository(state.repo.as_ref())?;
    git_operations::revert_commit(repo, &sha).map_err(AppError::Git)
}

#[tauri::command]
fn discard_all_changes(state: State<'_, App>) -> AppResult<()> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::discard_all_changes(repo).map_err(AppError::Git)
}

#[tauri::command]
fn get_settings(state: State<'_, App>) -> AppResult<SettingsPayload> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    settings_payload_from_settings(state.settings.clone())
}

#[tauri::command]
fn save_settings(
    state: State<'_, App>,
    app_handle: tauri::AppHandle,
    settings: SettingsPayload,
) -> AppResult<()> {
    let mut state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let previous_key_path = state.settings.ssh_key_path.clone();
    persist_passphrase(
        previous_key_path.as_deref(),
        settings.settings.ssh_key_path.as_deref(),
        settings.ssh_passphrase.as_deref(),
    )?;
    state.settings = settings.settings;
    save_settings_to_disk(&state, &app_handle)?;
    Ok(())
}

#[tauri::command]
fn set_remote_url(state: State<'_, App>, name: String, url: String) -> AppResult<()> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    let repo = state.repo.as_ref().ok_or(AppError::Git("No repository open".to_string()))?;
    git_operations::set_remote_url(repo, &name, &url).map_err(AppError::Git)
}

#[tauri::command]
fn get_remote_url(state: State<'_, App>, name: String) -> AppResult<String> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
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
        let mut state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
        state.settings.recent_repositories.retain(|p| !to_remove.contains(p));
        let _ = save_settings_to_disk(&state, &app_handle);
    }

    Ok(results)
}

#[tauri::command]
fn get_current_repo_info(state: State<'_, App>) -> AppResult<Option<RepositoryInfo>> {
    let state = state.0.lock().map_err(|_| AppError::Lock("Failed to acquire lock".to_string()))?;
    if let Some(repo) = state.repo.as_ref() {
        let info = git_operations::get_repository_info(repo).map_err(AppError::Git)?;
        Ok(Some(info))
    } else {
        Ok(None)
    }
}

#[tauri::command]
fn reveal_in_finder(path: String) -> AppResult<()> {
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
            app.manage(App(Mutex::new(AppState {
                repo,
                settings,
                watcher,
            })));

            // Cleanup on app exit
            let app_handle_clone = app_handle.clone();
            app.listen("tauri://close-requested", move |_| {
                if let Some(state) = app_handle_clone.try_state::<App>() {
                    if let Ok(mut app_state) = state.0.lock() {
                        stop_watcher(app_state.watcher.take());
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
