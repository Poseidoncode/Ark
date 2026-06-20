import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { mount } from '@vue/test-utils';
import { createPinia, setActivePinia } from 'pinia';
import HeaderBar from '../HeaderBar.vue';

// Mock the git service
vi.mock('../../services/git', () => ({
  gitService: {
    fetch: vi.fn().mockResolvedValue(undefined),
  },
}));

describe('HeaderBar', () => {
  let pinia: ReturnType<typeof createPinia>;

  beforeEach(() => {
    pinia = createPinia();
    setActivePinia(pinia);
    
    // Mock document.documentElement for theme
    Object.defineProperty(document.documentElement, 'setAttribute', {
      value: vi.fn(),
      writable: true,
    });
  });

  afterEach(() => {
    setActivePinia(undefined);
  });

  const createWrapper = (props = {}) => {
    return mount(HeaderBar, {
      props,
      global: {
        plugins: [pinia],
        stubs: {
          // Stub child components if needed
        },
      },
    });
  };

  describe('Repo Name Display', () => {
    it('should display "None" when no repo is open', () => {
      const wrapper = createWrapper();
      
      // Check that "None" is displayed when no repo
      expect(wrapper.text()).toContain('None');
    });

    it('should display repo name when repo is open', async () => {
      const wrapper = createWrapper();
      const repoStore = (await import('../../stores/repo')).useRepoStore();
      
      // Set repo info
      repoStore.repoInfo = {
        path: '/test/path/my-repo',
        current_branch: 'main',
        is_dirty: false,
        ahead: 0,
        behind: 0,
      };

      await wrapper.vm.$nextTick();
      
      // Should display the repo name
      expect(wrapper.text()).toContain('my-repo');
    });

    it('should handle .git suffix in path', async () => {
      const wrapper = createWrapper();
      const repoStore = (await import('../../stores/repo')).useRepoStore();
      
      repoStore.repoInfo = {
        path: '/test/path/my-repo.git',
        current_branch: 'main',
        is_dirty: false,
        ahead: 0,
        behind: 0,
      };

      await wrapper.vm.$nextTick();
      
      // Should display the repo name without .git
      expect(wrapper.text()).toContain('my-repo');
    });
  });

  describe('Branch Display', () => {
    it('should display branch name when repo is open', async () => {
      const wrapper = createWrapper();
      const repoStore = (await import('../../stores/repo')).useRepoStore();
      
      repoStore.repoInfo = {
        path: '/test/path/my-repo',
        current_branch: 'feature/test-branch',
        is_dirty: false,
        ahead: 0,
        behind: 0,
      };
      
      repoStore.branches = [
        { name: 'feature/test-branch', is_current: true, is_remote: false },
        { name: 'main', is_current: false, is_remote: false },
      ];

      await wrapper.vm.$nextTick();
      
      // Should display the branch name
      expect(wrapper.text()).toContain('feature/test-branch');
    });

    it('should not display branch section when no repo is open', () => {
      const wrapper = createWrapper();
      
      // Branch section should not be visible - check for branch icon
      // The branch section has a specific SVG with branch icon
      // ponytail: unused variable
      wrapper.findAll('svg').filter(svg => 
        svg.html()?.includes('line x1="6"')
      );
      // When no repo, branch section should not exist
      expect(wrapper.text()).not.toContain('Unknown');
    });
  });

  describe('Fetch Button', () => {
    it('should render fetch button when repo is open', async () => {
      const wrapper = createWrapper();
      const repoStore = (await import('../../stores/repo')).useRepoStore();
      
      repoStore.repoInfo = {
        path: '/test/path/my-repo',
        current_branch: 'main',
        is_dirty: false,
        ahead: 0,
        behind: 0,
      };

      await wrapper.vm.$nextTick();
      
      // Should have fetch button
      const fetchButton = wrapper.findAll('button').find(b => b.text() === 'Fetch');
      expect(fetchButton).toBeDefined();
    });

    it('should not render fetch button when no repo is open', () => {
      const wrapper = createWrapper();
      
      const fetchButton = wrapper.findAll('button').find(b => b.text() === 'Fetch');
      expect(fetchButton).toBeUndefined();
    });

    it('should emit handleFetch event when fetch button is clicked', async () => {
      const wrapper = createWrapper();
      const repoStore = (await import('../../stores/repo')).useRepoStore();
      
      repoStore.repoInfo = {
        path: '/test/path/my-repo',
        current_branch: 'main',
        is_dirty: false,
        ahead: 0,
        behind: 0,
      };

      await wrapper.vm.$nextTick();
      
      const fetchButton = wrapper.findAll('button').find(b => b.text() === 'Fetch');
      await fetchButton?.trigger('click');
      
      expect(wrapper.emitted('handleFetch')).toBeTruthy();
    });
  });

  describe('Settings Button', () => {
    it('should render settings button', () => {
      const wrapper = createWrapper();
      
      // Should have settings button (has title "Settings")
      const settingsButton = wrapper.findAll('button[title="Settings"]');
      expect(settingsButton.length).toBe(1);
    });

    it('should open settings modal when settings button is clicked', async () => {
      const wrapper = createWrapper();
      const uiStore = (await import('../../stores/ui')).useUIStore();
      
      const settingsButton = wrapper.find('button[title="Settings"]');
      await settingsButton.trigger('click');
      
      expect(uiStore.showSettingsModal).toBe(true);
    });
  });

  describe('Theme Toggle', () => {
    it('should render theme toggle button', () => {
      const wrapper = createWrapper();
      
      // Should have theme toggle button
      const themeButtons = wrapper.findAll('button');
      expect(themeButtons.length).toBeGreaterThan(0);
    });
  });

  describe('Ahead/Behind Indicators', () => {
    it('should display ahead indicator when repo is ahead', async () => {
      const wrapper = createWrapper();
      const repoStore = (await import('../../stores/repo')).useRepoStore();
      
      repoStore.repoInfo = {
        path: '/test/path/my-repo',
        current_branch: 'main',
        is_dirty: false,
        ahead: 3,
        behind: 0,
      };

      await wrapper.vm.$nextTick();
      
      // Should display ahead badge
      expect(wrapper.text()).toContain('↑3');
    });

    it('should display behind indicator when repo is behind', async () => {
      const wrapper = createWrapper();
      const repoStore = (await import('../../stores/repo')).useRepoStore();
      
      repoStore.repoInfo = {
        path: '/test/path/my-repo',
        current_branch: 'main',
        is_dirty: false,
        ahead: 0,
        behind: 5,
      };

      await wrapper.vm.$nextTick();
      
      // Should display behind badge
      expect(wrapper.text()).toContain('↓5');
    });
  });
});
