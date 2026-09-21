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
  error?: string | null;
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
  truncated: boolean;
}

export interface RepoSnapshot {
  status: FileStatus[];
  branches: BranchInfo[];
  conflicts: ConflictInfo[];
  info: RepositoryInfo;
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

export interface DeviceFlowInfo {
  user_code: string;
  verification_uri: string;
  device_code: string;
  interval: number;
  expires_in: number;
}

export interface GitHubUser {
  login: string;
  name: string | null;
  email: string | null;
}

/**
 * Simple cache for local git operations
 */
type CacheValue<T> = { data: T; until: number };

class GitService {
  private cache = new Map<string, CacheValue<unknown>>();
  private pending = new Map<string, Promise<unknown>>();
  private epoch = 0;
  private static SHORT_TTL = 5000;  // status, branches, history
  private static MEDIUM_TTL = 15000;  // conflicts (stashes uncached)
  private static LONG_TTL = 60000;   // settings, tags, remotes, commit diffs
  private static MAX_CACHE_SIZE = 100;
  // Per-partition key cap so one namespace (e.g. per-file diffs) cannot
  // evict everything else. Partition = key segment before the first ':'.
  private static MAX_PER_PARTITION = 30;

  private partitionOf(key: string): string {
    const i = key.indexOf(':');
    return i < 0 ? key : key.slice(0, i);
  }

  /** Refresh recency so hits count as recently used (LRU, not FIFO). */
  private touch(key: string, entry: CacheValue<unknown>): void {
    this.cache.delete(key);
    this.cache.set(key, entry);
  }

  /** Evict the LRU key within the partition first, else the global LRU key. */
  private evictFor(partition: string): void {
    let count = 0;
    let oldestInPartition: string | undefined;
    for (const k of this.cache.keys()) {
      if (this.partitionOf(k) === partition) {
        count++;
        if (oldestInPartition === undefined) oldestInPartition = k;
      }
    }
    if (oldestInPartition !== undefined && count >= GitService.MAX_PER_PARTITION) {
      this.cache.delete(oldestInPartition);
      return;
    }
    if (this.cache.size >= GitService.MAX_CACHE_SIZE) {
      const oldestKey = this.cache.keys().next().value;
      if (oldestKey) this.cache.delete(oldestKey);
    }
  }

  private async cached<T>(key: string, fetch: () => Promise<T>, ttl = GitService.SHORT_TTL): Promise<T> {
    const now = Date.now();
    const entry = this.cache.get(key) as CacheValue<T> | undefined;
    if (entry && entry.until > now) {
      this.touch(key, entry);
      return entry.data;
    }
    // Same-key in-flight dedup: concurrent callers share one promise.
    const inFlight = this.pending.get(key) as Promise<T> | undefined;
    if (inFlight) return inFlight;
    const startEpoch = this.epoch;
    let task!: Promise<T>;
    task = (async (): Promise<T> => {
      try {
        const data = await fetch();
        // Skip caching when an invalidation landed mid-flight so a
        // pre-mutation response cannot repopulate the cache as fresh.
        if (this.epoch !== startEpoch) return data;
        this.evictFor(this.partitionOf(key));
        this.cache.set(key, { data, until: Date.now() + ttl });
        return data;
      } catch (error) {
        // Only fall back to cache if the entry is still within its TTL.
        // An expired entry is never served: it would mask real errors and
        // can leak data from a previous repository state (e.g. phantom
        // stashes). Expired entries are dropped so the error surfaces.
        if (entry && entry.until > Date.now()) {
          console.warn('Cache fallback for ' + key + ':', error);
          return entry.data;
        }
        if (entry) this.cache.delete(key);
        throw error;
      } finally {
        if (this.pending.get(key) === task) this.pending.delete(key);
      }
    })();
    this.pending.set(key, task);
    return task;
  }

  invalidate(prefix?: string): void {
    if (!prefix) { this.cache.clear(); this.pending.clear(); this.epoch++; return; }
    let dropped = false;
    for (const key of this.cache.keys()) {
      if (key.startsWith(prefix)) { this.cache.delete(key); dropped = true; }
    }
    for (const key of this.pending.keys()) {
      if (key.startsWith(prefix)) { this.pending.delete(key); dropped = true; }
    }
    if (dropped) this.epoch++;
  }

  /**
   * Clone repository
   */
  async cloneRepository(url: string, path: string): Promise<string> {
    this.invalidate();
    return await invoke("clone_repository", { options: { url, path } });
  }

  /**
   * Open repository with caching
   */
  async openRepository(path: string): Promise<RepositoryInfo> {
    this.invalidate();
    return await invoke("open_repository", { path });
  }

  /**
   * Get repository status
   */
  async getStatus(): Promise<FileStatus[]> {
    return await this.cached('repo:status', () => invoke("get_repository_status"), GitService.SHORT_TTL);
  }

  /**
   * Single-IPC snapshot of status/branches/conflicts/repoInfo, computed by
   * the backend with one repository handle and one status scan.
   * Any mutation that previously invalidated one of the covered sections
   * must also invalidate 'repo:snapshot'.
   */
  async getRepoSnapshot(): Promise<RepoSnapshot> {
    return await this.cached('repo:snapshot', () => invoke("get_repo_snapshot"), GitService.SHORT_TTL);
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
    this.invalidate('repo:snapshot');
    this.invalidate('diff:');
    return await invoke("stage_files", { files });
  }

  /**
   * Unstage files with cache invalidation
   */
  async unstageFiles(files: string[]): Promise<void> {
    this.invalidate('repo:status');
    this.invalidate('repo:snapshot');
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
    return await this.cached('repo:branches', () => invoke("get_branches"), GitService.SHORT_TTL);
  }

  /**
   * Create branch
   */
  async createBranch(name: string, startSha?: string): Promise<void> {
    this.invalidate('repo:branches');
    this.invalidate('repo:snapshot');
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
    return await this.cached(`commit:diff:${sha}`, () => invoke("get_commit_diff", { sha }), GitService.LONG_TTL);
  }

  /**
   * Get commit history with limit/offset pagination. The backend walks from
   * HEAD with a limit, so offset pages are served client-side from a
   * (limit + offset) window and cached per page.
   */
  async getHistory(limit: number = 200, offset: number = 0): Promise<CommitInfo[]> {
    const safeLimit = Math.max(0, Math.min(Math.floor(limit), 500));
    const safeOffset = Math.max(0, Math.floor(offset));
    return await this.cached(
      `repo:history:${safeLimit}:${safeOffset}`,
      async () => {
        const all = (await invoke("get_commit_history", {
          limit: Math.min(safeLimit + safeOffset, 500),
        })) as CommitInfo[];
        return all.slice(safeOffset, safeOffset + safeLimit);
      },
      GitService.SHORT_TTL,
    );
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
    this.invalidate('repo:snapshot');
    return await invoke("stash_save", { options: { message } });
  }

  /**
   * Stash pop
   */
  async stashPop(sha: string): Promise<void> {
    this.invalidate('repo:');
    return await invoke("stash_pop", { sha });
  }

  /**
   * List stashes with limit/offset pagination (applied client-side).
   * Stash list must always reflect the real repository state.
   * External tools (terminal, other GUIs) can modify stashes at any time,
   * and the file watcher will trigger refreshes — caching here would serve
   * stale data that makes ARK show non-existent stashes.
   */
  async listStashes(limit?: number, offset: number = 0): Promise<StashInfo[]> {
    const all = (await invoke("list_stashes")) as StashInfo[];
    const safeOffset = Math.max(0, Math.floor(offset));
    if (limit === undefined) return safeOffset === 0 ? all : all.slice(safeOffset);
    const safeLimit = Math.max(0, Math.floor(limit));
    return all.slice(safeOffset, safeOffset + safeLimit);
  }

  /**
   * Get conflicts
   */
  async getConflicts(): Promise<ConflictInfo[]> {
    return await this.cached('repo:conflicts', () => invoke("get_conflicts"), GitService.MEDIUM_TTL);
  }

  /**
   * Resolve conflict
   */
  async resolveConflict(path: string, useOurs: boolean): Promise<void> {
    this.invalidate('repo:conflicts');
    this.invalidate('repo:status');
    this.invalidate('repo:snapshot');
    this.invalidate('diff:');
    return await invoke("resolve_conflict", { path, useOurs });
  }

  /**
   * Get settings
   */
  async getSettings(): Promise<SettingsPayload> {
    return await this.cached('settings', () => invoke("get_settings"), GitService.LONG_TTL);
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
    this.invalidate('repo:remotes');
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
    this.invalidate('repo:status');
    this.invalidate('repo:snapshot');
    return await invoke("add_to_gitignore", { filePath });
  }

  /**
   * Resolve a repo-relative path to a validated absolute path.
   * Must be called before opening files with the OS.
   */
  async resolveRepoFile(filePath: string): Promise<string> {
    return await invoke("resolve_repo_file", { filePath });
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
  async dropStash(sha: string): Promise<void> {
    this.invalidate('repo:stash');
    return await invoke("drop_stash", { sha });
  }

  /**
   * Apply stash
   */
  async applyStash(sha: string): Promise<void> {
    this.invalidate('repo:');
    return await invoke("apply_stash", { sha });
  }

  /**
   * Branch from stash
   */
  async branchFromStash(sha: string, branchName: string): Promise<void> {
    this.invalidate('repo:branches');
    this.invalidate('repo:stash');
    this.invalidate('repo:status');
    this.invalidate('repo:snapshot');
    return await invoke("branch_from_stash", { sha, branchName });
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
    return await this.cached('repo:tags', () => invoke("list_tags"), GitService.LONG_TTL);
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
    return await this.cached('repo:remotes', () => invoke("list_remotes"), GitService.LONG_TTL);
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

  // ── GitHub OAuth Device Flow ────────────────────────────────

  /**
   * Check if GitHub OAuth is enabled (client ID configured at build time).
   */
  async oauthIsEnabled(): Promise<boolean> {
    return await invoke("oauth_is_enabled");
  }

  /**
   * Start the OAuth Device Flow — request a device code from GitHub.
   * Returns the user code and verification URI to display to the user.
   */
  async oauthStartDeviceFlow(): Promise<DeviceFlowInfo> {
    return await invoke("oauth_start_device_flow");
  }

  /**
   * Poll GitHub for an access token. Blocks until the user approves
   * or the device code expires. Returns the authenticated user's profile.
   */
  async oauthPollForToken(deviceCode: string, interval: number, expiresIn: number): Promise<GitHubUser> {
    return await invoke("oauth_poll_for_token", {
      deviceCode, interval, expiresIn
    });
  }

  /**
   * Check if the user is authenticated with GitHub.
   * Returns the GitHub user info if authenticated, or null if not.
   */
  async oauthGetStatus(): Promise<GitHubUser | null> {
    return await invoke("oauth_get_status");
  }

  /**
   * Sign out — delete the stored OAuth token.
   */
  async oauthSignOut(): Promise<void> {
    return await invoke("oauth_sign_out");
  }

  /**
   * Cancel an in-progress OAuth polling loop.
   */
  async oauthCancelPolling(): Promise<void> {
    return await invoke("oauth_cancel_polling");
  }
}

// Export singleton instance
export const gitService = new GitService();