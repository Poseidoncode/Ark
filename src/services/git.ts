import { invoke } from "@tauri-apps/api/core";

/**
 * Cache entry with expiration
 */
interface CacheEntry<T> {
  data: T;
  expiresAt: number;
}

/**
 * Debounced function entry
 */
interface DebouncedEntry {
  id: ReturnType<typeof setTimeout>;
  resolve: (value: unknown) => void;
  reject: (error: Error) => void;
}

/**
 * High-performance Git service with caching and debouncing
 * Reduces unnecessary Rust backend calls for better performance
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

class GitServiceOptimizer {
  private cache = new Map<string, CacheEntry<unknown>>();
  private debounceTimers = new Map<string, DebouncedEntry>();
  private readonly DEFAULT_TTL = 5000; // 5 seconds
  private readonly SHORT_TTL = 1000; // 1 second for frequently changing data
  private readonly LONG_TTL = 30000; // 30 seconds for static data
  private readonly MAX_CACHE_SIZE = 50; // Memory optimization

  /**
   * Memory cleanup: Evict old cache entries when limit is reached
   */
  private evictOldCache() {
    if (this.cache.size > this.MAX_CACHE_SIZE) {
      const entries = Array.from(this.cache.entries());
      entries.sort((a, b) => a[1].expiresAt - b[1].expiresAt);
      // Remove oldest 25%
      const toRemove = Math.floor(this.MAX_CACHE_SIZE * 0.25);
      for (let i = 0; i < toRemove; i++) {
        this.cache.delete(entries[i][0]);
      }
    }
  }

  /**
   * Get cached data or fetch from backend
   */
  private async getCachedOrFetch<T>(
    key: string,
    fetchFn: () => Promise<T>,
    ttl: number = this.DEFAULT_TTL
  ): Promise<T> {
    const now = Date.now();
    const cached = this.cache.get(key) as CacheEntry<T> | undefined;

    if (cached && cached.expiresAt > now) {
      return cached.data;
    }

    try {
      const data = await fetchFn();
      this.cache.set(key, { data, expiresAt: now + ttl });
      this.evictOldCache(); // Memory optimization
      return data;
    } catch (error) {
      // On error, try to return stale cache if available
      const cached = this.cache.get(key) as CacheEntry<T> | undefined;
      if (cached) {
        console.warn(`Cache fallback for ${key}:`, error);
        return cached.data;
      }
      throw error;
    }
  }

  /**
   * Debounce rapid calls for the same key
   */
  private debounce<T>(
    key: string,
    fn: () => Promise<T>,
    delay: number = 300
  ): Promise<T> {
    const existing = this.debounceTimers.get(key);

    if (existing) {
      clearTimeout(existing.id);
    }

    return new Promise((resolve, reject) => {
      const id = setTimeout(async () => {
        this.debounceTimers.delete(key);
        try {
          const result = await fn();
          resolve(result);
        } catch (error) {
          reject(error as Error);
        }
      }, delay);

      this.debounceTimers.set(key, { id, resolve: resolve as (value: unknown) => void, reject });
    });
  }

  /**
   * Invalidate cache for specific keys
   */
  invalidate(keyPattern?: string): void {
    if (!keyPattern) {
      this.cache.clear();
      return;
    }

    for (const key of this.cache.keys()) {
      if (key.includes(keyPattern)) {
        this.cache.delete(key);
      }
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
    const cacheKey = `repo:info:${path}`;
    return await this.getCachedOrFetch(
      cacheKey,
      async () => {
        this.invalidate('repo:');
        return await invoke("open_repository", { path });
      },
      this.SHORT_TTL
    );
  }

  /**
   * Get repository status with debouncing
   */
  async getStatus(): Promise<FileStatus[]> {
    const cacheKey = 'repo:status';
    const result = await this.debounce(
      cacheKey,
      () => this.getCachedOrFetch(cacheKey, () => invoke("get_repository_status"), this.SHORT_TTL),
      150
    );
    return result as FileStatus[];
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
    return await invoke("stage_files", { files });
  }

  /**
   * Unstage files with cache invalidation
   */
  async unstageFiles(files: string[]): Promise<void> {
    this.invalidate('repo:status');
    return await invoke("unstage_files", { files });
  }

  /**
   * Discard changes
   */
  async discardChanges(filePath: string): Promise<void> {
    this.invalidate('repo:');
    return await invoke("discard_changes", { filePath });
  }

  /**
   * Discard all changes
   */
  async discardAllChanges(): Promise<void> {
    this.invalidate('repo:');
    return await invoke("discard_all_changes");
  }

  /**
   * Get branches with caching
   */
  async getBranches(): Promise<BranchInfo[]> {
    const cacheKey = 'repo:branches';
    return await this.getCachedOrFetch(
      cacheKey,
      () => invoke("get_branches"),
      this.DEFAULT_TTL
    );
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
    const cacheKey = `commit:diff:${sha.substring(0, 7)}`;
    return await this.getCachedOrFetch(
      cacheKey,
      () => invoke("get_commit_diff", { sha }),
      this.LONG_TTL
    );
  }

  /**
   * Get commit history
   */
  async getHistory(limit: number = 50): Promise<CommitInfo[]> {
    const cacheKey = `repo:history:${limit}`;
    return await this.getCachedOrFetch(
      cacheKey,
      () => invoke("get_commit_history", { limit }),
      this.SHORT_TTL
    );
  }

  /**
   * Get diff
   */
  async getDiff(filePath?: string): Promise<DiffInfo[]> {
    const cacheKey = filePath ? `diff:${filePath}` : 'diff:all';
    const result = await this.debounce(
      cacheKey,
      () => invoke("get_diff", { filePath }),
      200
    );
    return result as DiffInfo[];
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
    this.invalidate('repo:ahead');
    return await invoke("fetch_changes");
  }

  /**
   * Stash save
   */
  async stashSave(message?: string): Promise<void> {
    this.invalidate('repo:stash');
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
    const cacheKey = 'repo:stashes';
    return await this.getCachedOrFetch(
      cacheKey,
      () => invoke("list_stashes"),
      this.DEFAULT_TTL
    );
  }

  /**
   * Get conflicts
   */
  async getConflicts(): Promise<ConflictInfo[]> {
    const cacheKey = 'repo:conflicts';
    return await this.getCachedOrFetch(
      cacheKey,
      () => invoke("get_conflicts"),
      this.SHORT_TTL
    );
  }

  /**
   * Resolve conflict
   */
  async resolveConflict(path: string, useOurs: boolean): Promise<void> {
    this.invalidate('repo:conflicts');
    this.invalidate('repo:status');
    return await invoke("resolve_conflict", { path, useOurs });
  }

  /**
   * Get settings
   */
  async getSettings(): Promise<SettingsPayload> {
    const cacheKey = 'settings';
    return await this.getCachedOrFetch(
      cacheKey,
      () => invoke("get_settings"),
      this.LONG_TTL
    );
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
    const cacheKey = 'repo:tags';
    return await this.getCachedOrFetch(
      cacheKey,
      () => invoke("list_tags"),
      this.LONG_TTL
    );
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
    const cacheKey = 'repo:remotes';
    return await this.getCachedOrFetch(
      cacheKey,
      () => invoke("list_remotes"),
      this.LONG_TTL
    );
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
export const gitService = new GitServiceOptimizer();