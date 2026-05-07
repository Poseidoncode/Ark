import { defineStore } from 'pinia';
import { ref } from 'vue';
import type { TagInfo, RemoteInfo } from '../services/git';

export type ViewType = 'changes' | 'history' | 'stashes' | 'conflicts';

export const useUIStore = defineStore('ui', () => {
  // View state
  const view = ref<ViewType>('changes');
  
  // Loading state
  const loading = ref(false);
  const loadingMessage = ref('');
  const isMajorOperation = ref(false);
  
  // Error state
  const error = ref<string | null>(null);
  const lastFailedOperation = ref<(() => Promise<void>) | null>(null);
  
  // Modal states
  const showCloneModal = ref(false);
  const cloneUrl = ref('');
  const clonePath = ref('');
  const showSettingsModal = ref(false);
  const showBranchModal = ref(false);
  const newBranchName = ref('');
  const showRecentRepos = ref(false);
  const showTagsModal = ref(false);
  const showRemotesModal = ref(false);
  
  // Tags and remotes
  const tags = ref<TagInfo[]>([]);
  const remotes = ref<RemoteInfo[]>([]);
  const newRemoteName = ref('');
  const newRemoteUrl = ref('');
  
  // Commit form state
  const commitMessage = ref('');
  const amendCommit = ref(false);
  const searchCommitQuery = ref('');

  // Actions
  const setError = (err: string | null) => {
    error.value = err;
  };

  const clearError = () => {
    error.value = null;
    lastFailedOperation.value = null;
  };

  const setLoading = (isLoading: boolean, message = '', major = false) => {
    loading.value = isLoading;
    loadingMessage.value = message;
    isMajorOperation.value = major;
  };

  const openModal = (modal: 'clone' | 'settings' | 'branch' | 'tags' | 'remotes' | 'recentRepos') => {
    switch (modal) {
      case 'clone':
        showCloneModal.value = true;
        cloneUrl.value = '';
        clonePath.value = '';
        break;
      case 'settings':
        showSettingsModal.value = true;
        break;
      case 'branch':
        showBranchModal.value = true;
        break;
      case 'tags':
        showTagsModal.value = true;
        break;
      case 'remotes':
        showRemotesModal.value = true;
        break;
      case 'recentRepos':
        showRecentRepos.value = true;
        break;
    }
  };

  const closeModal = (modal: 'clone' | 'settings' | 'branch' | 'tags' | 'remotes' | 'recentRepos') => {
    switch (modal) {
      case 'clone':
        showCloneModal.value = false;
        break;
      case 'settings':
        showSettingsModal.value = false;
        break;
      case 'branch':
        showBranchModal.value = false;
        break;
      case 'tags':
        showTagsModal.value = false;
        break;
      case 'remotes':
        showRemotesModal.value = false;
        break;
      case 'recentRepos':
        showRecentRepos.value = false;
        break;
    }
  };

  const closeAllModals = () => {
    showCloneModal.value = false;
    showSettingsModal.value = false;
    showBranchModal.value = false;
    showTagsModal.value = false;
    showRemotesModal.value = false;
  };

  const setView = (newView: ViewType) => {
    view.value = newView;
  };

  const setCommitMessage = (message: string) => {
    commitMessage.value = message;
  };

  const setAmendCommit = (value: boolean) => {
    amendCommit.value = value;
  };

  const setSearchCommitQuery = (query: string) => {
    searchCommitQuery.value = query;
  };

  const setTags = (newTags: TagInfo[]) => {
    tags.value = newTags;
  };

  const setRemotes = (newRemotes: RemoteInfo[]) => {
    remotes.value = newRemotes;
  };

  const setCloneUrl = (url: string) => {
    cloneUrl.value = url;
  };

  const setClonePath = (path: string) => {
    clonePath.value = path;
  };

  const setNewBranchName = (name: string) => {
    newBranchName.value = name;
  };

  const setNewRemoteName = (name: string) => {
    newRemoteName.value = name;
  };

  const setNewRemoteUrl = (url: string) => {
    newRemoteUrl.value = url;
  };

  const clearNewRemoteFields = () => {
    newRemoteName.value = '';
    newRemoteUrl.value = '';
  };

  return {
    // State
    view,
    loading,
    loadingMessage,
    isMajorOperation,
    error,
    lastFailedOperation,
    showCloneModal,
    cloneUrl,
    clonePath,
    showSettingsModal,
    showBranchModal,
    newBranchName,
    showRecentRepos,
    showTagsModal,
    showRemotesModal,
    tags,
    remotes,
    newRemoteName,
    newRemoteUrl,
    commitMessage,
    amendCommit,
    searchCommitQuery,
    
    // Actions
    setError,
    clearError,
    setLoading,
    openModal,
    closeModal,
    closeAllModals,
    setView,
    setCommitMessage,
    setAmendCommit,
    setSearchCommitQuery,
    setTags,
    setRemotes,
    setCloneUrl,
    setClonePath,
    setNewBranchName,
    setNewRemoteName,
    setNewRemoteUrl,
    clearNewRemoteFields,
  };
});
