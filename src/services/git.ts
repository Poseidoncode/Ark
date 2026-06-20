import { invoke } from "@tauri-apps/api/core";

export interface RepositoryInfo {
  path: string;
  current_branch: string;
  is_dirty: boolean;
  ahead: number;
  behind: number;
}

export type FileStatusKind = 'added' | 'modified' | 'deleted' | 'untracked' | 'unknown';

export interface FileStatus {
  path: string;
  status: FileStatusKind;
  staged: boolean;
}

export interface CommitInfo {
  sha: string;
  message: string;
  author: string;
  email: string;
  timestamp: number;
  is_pushed: boolean;
  parents: string[];
}

export interface BranchInfo {
  name: string;
  is_current: boolean;
  is_remote: boolean;
}

export interface DiffInfo {
  path: string;
  additions: number;
  deletions: number;
  diff_text: string;
}

export interface StashInfo {
  index: number;
  message: string;
  sha: string;
}

export interface ConflictInfo {
  path: string;
  our_status: string;
  their_status: string;
}

export interface Settings {
  user_name: string;
  user_email: string;
  ssh_key_path: string | null;
  theme: string;
  recent_repositories: string[];
  last_opened_repository: string | null;
}

export interface SettingsPayload extends Settings {
  ssh_passphrase: string | null;
}

export interface StageResult {
  staged: string[];
  warnings: string[];
}

export interface TagInfo {
  name: string;
  message: string | null;
  sha: string;
  date: number;
}

export interface RemoteInfo {
  name: string;
  url: string;
  fetch_url: string | null;
}

class GitService {
  async openRepository(path: string): Promise<RepositoryInfo> {
    return await invoke("open_repository", { path });
  }

  async cloneRepository(url: string, path: string): Promise<string> {
    return await invoke("clone_repository", { options: { url, path } });
  }

  async getStatus(): Promise<FileStatus[]> {
    return await invoke("get_repository_status");
  }

  async getBranches(): Promise<BranchInfo[]> {
    return await invoke("get_branches");
  }

  async createBranch(name: string, startSha?: string): Promise<void> {
    return await invoke("create_branch", { options: { name, start_sha: startSha } });
  }

  async checkoutBranch(name: string): Promise<void> {
    return await invoke("checkout_branch", { options: { name } });
  }

  async getCommitDiff(sha: string): Promise<DiffInfo[]> {
    return await invoke("get_commit_diff", { sha });
  }

  async getHistory(limit: number = 50): Promise<CommitInfo[]> {
    return await invoke("get_commit_history", { limit });
  }

  async getDiff(filePath?: string): Promise<DiffInfo[]> {
    return await invoke("get_diff", { filePath: filePath || null });
  }

  async stageFiles(files: string[]): Promise<StageResult> {
    return await invoke("stage_files", { files });
  }

  async unstageFiles(files: string[]): Promise<void> {
    return await invoke("unstage_files", { files });
  }

  async createCommit(message: string, files: string[]): Promise<string> {
    return await invoke("create_commit", { options: { message, files } });
  }

  async amendCommit(message: string): Promise<string> {
    return await invoke("amend_commit", { message });
  }

  async cherryPick(sha: string): Promise<void> {
    return await invoke("cherry_pick", { sha });
  }

  async revertCommit(sha: string): Promise<void> {
    return await invoke("revert_commit", { sha });
  }

  async discardChanges(filePath: string): Promise<void> {
    return await invoke("discard_changes", { filePath });
  }

  async discardAllChanges(): Promise<void> {
    return await invoke("discard_all_changes");
  }

  async push(): Promise<void> {
    return await invoke("push_changes");
  }

  async pull(): Promise<void> {
    return await invoke("pull_changes");
  }

  async fetch(): Promise<void> {
    return await invoke("fetch_changes");
  }

  async stashSave(message?: string): Promise<void> {
    return await invoke("stash_save", { options: { message } });
  }

  async stashPop(index: number): Promise<void> {
    return await invoke("stash_pop", { index });
  }

  async listStashes(): Promise<StashInfo[]> {
    return await invoke("list_stashes");
  }

  async getConflicts(): Promise<ConflictInfo[]> {
    return await invoke("get_conflicts");
  }

  async resolveConflict(path: string, useOurs: boolean): Promise<void> {
    return await invoke("resolve_conflict", { path, useOurs });
  }

  async getSettings(): Promise<SettingsPayload> {
    return await invoke("get_settings");
  }

  async saveSettings(settings: SettingsPayload): Promise<void> {
    return await invoke("save_settings", { settings });
  }

  async setRemoteUrl(name: string, url: string): Promise<void> {
    return await invoke("set_remote_url", { name, url });
  }

  async getRemoteUrl(name: string = "origin"): Promise<string> {
    return await invoke("get_remote_url", { name });
  }

  async getCurrentRepoInfo(): Promise<RepositoryInfo | null> {
    return await invoke("get_current_repo_info");
  }

  async getRepositoriesInfo(paths: string[]): Promise<RepositoryInfo[]> {
    return await invoke("get_repositories_info", { paths });
  }

  async revealInFinder(path: string): Promise<void> {
    return await invoke("reveal_in_finder", { path });
  }

  async addToGitignore(filePath: string): Promise<void> {
    return await invoke("add_to_gitignore", { filePath });
  }

  async readFile(filePath: string): Promise<string> {
    return await invoke("read_file", { filePath });
  }

  async createTag(name: string, message: string, sha: string): Promise<void> {
    return await invoke("create_tag", { options: { name, message, sha } });
  }

  async dropStash(index: number): Promise<void> {
    return await invoke("drop_stash", { index });
  }

  async applyStash(index: number): Promise<void> {
    return await invoke("apply_stash", { index });
  }

  async branchFromStash(index: number, branchName: string): Promise<void> {
    return await invoke("branch_from_stash", { index, branchName });
  }

  async resetBranch(sha: string): Promise<void> {
    return await invoke("reset_branch", { sha });
  }

  async mergeCommit(sha: string): Promise<void> {
    return await invoke("merge_commit", { sha });
  }

  async listTags(): Promise<TagInfo[]> {
    return await invoke("list_tags");
  }

  async deleteTag(name: string): Promise<void> {
    return await invoke("delete_tag", { name });
  }

  async listRemotes(): Promise<RemoteInfo[]> {
    return await invoke("list_remotes");
  }

  async addRemote(name: string, url: string): Promise<void> {
    return await invoke("add_remote", { name, url });
  }

  async removeRemote(name: string): Promise<void> {
    return await invoke("remove_remote", { name });
  }
}

export const gitService = new GitService();
