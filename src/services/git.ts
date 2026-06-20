import { invoke } from "@tauri-apps/api/core";

/**
 * High-performance Git service with caching
 */
export interface RepositoryInfo {
  path: string;
  current_branch: string;
  is_dirty: boolean;
  ahead: number;
  behind: number;
}

export interface FileStatus {
  path: string;
  status: string;
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

/**
 * Simple cache for local git operations
 */
type CacheValue<T> = { data: T; until: number };

class GitService {
  private cache = new Map<string, CacheValue<unknown>>();
  private static TTL = 1500; // 1.5s — local git is fast

  private async cached<T>(key: string, fetch: () => Promise<T>): Promise<T> {
    const now = Date.now();
    const entry = this.cache.get(key) as CacheValue<T> | undefined;
    if (entry && entry.until > now) return entry.data;
    try {
      const data = await fetch();
      this.cache.set(key, { data, until: now + GitService.TTL });
      return data;
    } catch (error) {
      if (entry) {
        console.warn('Cache fallback for ' + key + ':', error);
        return entry.data;
      }
      throw error;
    }
  }

  invalidate(prefix?: string): void {
    if (!prefix) { this.cache.clear(); return; }
    for (const key of this.cache.keys()) {
      if (key.startsWith(prefix)) this.cache.delete(key);
    }
  }

  /**
   * Clone repository
   */
  async cloneRepository(url: string, path: string): Promise<string> {
    this.invalidate('repo:');
    return await invoke("clone_repository", { options: { url, path } });
  }

  /**
   * Open repository with caching
   */
  async openRepository(path: string): Promise<RepositoryInfo> {
    this.invalidate('repo:');
    return await invoke("open_repository", { path });
  }

  /**
   * Get repository status
   */
  async getStatus(): Promise<FileStatus[]> {
    return await this.cached('repo:status', () => invoke("get_repository_status"));
  }

  /**
   * Create commit
   */
  async createCommit(message: string, files: string[]): Promise<string> {
    this.invalidate('repo:');
    return await invoke("create_commit", { options: { message, files } });
  }

  /**
   * Amend last commit
   */
  async amendCommit(message: string): Promise<string> {
    this.invalidate('repo:');
    return await invoke("amend_commit", { message });
  }

  /**
   * Cherry-pick commit
   */
  async cherryPick(sha: string): Promise<void> {
    this.invalidate('repo:');
    return await invoke("cherry_pick", { sha });
  }

  /**
   * Revert commit
   */
  async revertCommit(sha: string): Promise<void> {
    this.invalidate('repo:');
    return await invoke("revert_commit", { sha });
  }

  /**
   * Stage files with cache invalidation
   */
  async stageFiles(files: string[]): Promise<StageResult> {
    this.invalidate('repo:status');
    this.invalidate('diff:');
    return await invoke("stage_files", { files });
  }

  /**
   * Unstage files with cache invalidation
   */
  async unstageFiles(files: string[]): Promise<void> {
    this.invalidate('repo:status');
    this.invalidate('diff:');
    return await invoke("unstage_files", { files });
  }

  /**
   * Discard changes
   */
  async discardChanges(filePath: string): Promise<void> {
    this.invalidate('repo:');
    this.invalidate('diff:');
    return await invoke("discard_changes", { filePath });
  }

  /**
   * Discard all changes
   */
  async discardAllChanges(): Promise<void> {
    this.invalidate('repo:');
    this.invalidate('diff:');
    return await invoke("discard_all_changes");
  }

  /**
   * Get branches with caching
   */
  async getBranches(): Promise<BranchInfo[]> {
    return await this.cached('repo:branches', () => invoke("get_branches"));
  }

  /**
   * Create branch
   */
  async createBranch(name: string, startSha?: string): Promise<void> {
    this.invalidate('repo:branches');
    return await invoke("create_branch", { options: { name, start_sha: startSha } });
  }

  /**
   * Checkout branch
   */
  async checkoutBranch(name: string): Promise<void> {
    this.invalidate('repo:');
    return await invoke("checkout_branch", { options: { name } });
  }

  /**
   * Get commit diff
   */
  async getCommitDiff(sha: string): Promise<DiffInfo[]> {
    return await this.cached(`commit:diff:${sha.substring(0, 7)}`, () => invoke("get_commit_diff", { sha }));
  }

  /**
   * Get commit history
   */
  async getHistory(limit: number = 200): Promise<CommitInfo[]> {
    return await this.cached(`repo:history:${limit}`, () => invoke("get_commit_history", { limit }));
  }

  /**
   * Get diff
   */
  async getDiff(filePath?: string): Promise<DiffInfo[]> {
    const cacheKey = filePath ? `diff:${filePath}` : 'diff:all';
    return await this.cached(cacheKey, () => invoke("get_diff", { filePath }));
  }

  /**
   * Push changes
   */
  async push(): Promise<void> {
    this.invalidate('repo:');
    return await invoke("push_changes");
  }

  /**
   * Pull changes
   */
  async pull(): Promise<void> {
    this.invalidate('repo:');
    return await invoke("pull_changes");
  }

  /**
   * Fetch changes
   */
  async fetch(): Promise<void> {
    this.invalidate('repo:');
    return await invoke("fetch_changes");
  }

  /**
   * Stash save
   */
  async stashSave(message?: string): Promise<void> {
    this.invalidate('repo:stash');
    this.invalidate('repo:status');
    return await invoke("stash_save", { options: { message } });
  }

  /**
   * Stash pop
   */
  async stashPop(index: number): Promise<void> {
    this.invalidate('repo:');
    return await invoke("stash_pop", { index });
  }

  /**
   * List stashes
   */
  async listStashes(): Promise<StashInfo[]> {
    return await this.cached('repo:stashes', () => invoke("list_stashes"));
  }

  /**
   * Get conflicts
   */
  async getConflicts(): Promise<ConflictInfo[]> {
    return await this.cached('repo:conflicts', () => invoke("get_conflicts"));
  }

  /**
   * Resolve conflict
   */
  async resolveConflict(path: string, useOurs: boolean): Promise<void> {
    this.invalidate('repo:conflicts');
    this.invalidate('repo:status');
    this.invalidate('diff:');
    return await invoke("resolve_conflict", { path, useOurs });
  }

  /**
   * Get settings
   */
  async getSettings(): Promise<SettingsPayload> {
    return await this.cached('settings', () => invoke("get_settings"));
  }

  /**
   * Save settings
   */
  async saveSettings(settings: SettingsPayload): Promise<void> {
    this.invalidate('settings');
    return await invoke("save_settings", { settings });
  }

  /**
   * Set remote URL
   */
  async setRemoteUrl(name: string, url: string): Promise<void> {
    return await invoke("set_remote_url", { name, url });
  }

  /**
   * Get remote URL
   */
  async getRemoteUrl(name: string = "origin"): Promise<string> {
    return await invoke("get_remote_url", { name });
  }

  /**
   * Get current repo info
   */
  async getCurrentRepoInfo(): Promise<RepositoryInfo | null> {
    return await invoke("get_current_repo_info");
  }

  /**
   * Get repositories info
   */
  async getRepositoriesInfo(paths: string[]): Promise<RepositoryInfo[]> {
    return await invoke("get_repositories_info", { paths });
  }

  /**
   * Reveal in finder
   */
  async revealInFinder(path: string): Promise<void> {
    return await invoke("reveal_in_finder", { path });
  }

  /**
   * Add to gitignore
   */
  async addToGitignore(filePath: string): Promise<void> {
    return await invoke("add_to_gitignore", { filePath });
  }

  /**
   * Read file
   */
  async readFile(filePath: string): Promise<string> {
    return await invoke("read_file", { filePath });
  }

  /**
   * Create tag
   */
  async createTag(name: string, message: string, sha: string): Promise<void> {
    return await invoke("create_tag", { options: { name, message, sha } });
  }

  /**
   * Drop stash
   */
  async dropStash(index: number): Promise<void> {
    this.invalidate('repo:stash');
    return await invoke("drop_stash", { index });
  }

  /**
   * Apply stash
   */
  async applyStash(index: number): Promise<void> {
    this.invalidate('repo:');
    return await invoke("apply_stash", { index });
  }

  /**
   * Branch from stash
   */
  async branchFromStash(index: number, branchName: string): Promise<void> {
    this.invalidate('repo:branches');
    return await invoke("branch_from_stash", { index, branchName });
  }

  /**
   * Reset branch
   */
  async resetBranch(sha: string): Promise<void> {
    this.invalidate('repo:');
    return await invoke("reset_branch", { sha });
  }

  /**
   * Merge commit
   */
  async mergeCommit(sha: string): Promise<void> {
    this.invalidate('repo:');
    return await invoke("merge_commit", { sha });
  }

  /**
   * List all tags
   */
  async listTags(): Promise<TagInfo[]> {
    return await this.cached('repo:tags', () => invoke("list_tags"));
  }

  /**
   * Delete a tag
   */
  async deleteTag(name: string): Promise<void> {
    this.invalidate('repo:tags');
    return await invoke("delete_tag", { name });
  }

  /**
   * List all remotes
   */
  async listRemotes(): Promise<RemoteInfo[]> {
    return await this.cached('repo:remotes', () => invoke("list_remotes"));
  }

  /**
   * Add a remote
   */
  async addRemote(name: string, url: string): Promise<void> {
    this.invalidate('repo:remotes');
    return await invoke("add_remote", { name, url });
  }

  /**
   * Remove a remote
   */
  async removeRemote(name: string): Promise<void> {
    this.invalidate('repo:remotes');
    return await invoke("remove_remote", { name });
  }
}

// Export singleton instance
export const gitService = new GitService();