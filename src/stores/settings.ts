import { defineStore } from 'pinia';
import { ref, watch } from 'vue';
import { gitService, type SettingsPayload } from '../services/git';

export const useSettingsStore = defineStore('settings', () => {
  // State
  const settings = ref<SettingsPayload | null>(null);
  const isLoaded = ref(false);

  // Actions
  const fetchSettings = async () => {
    try {
      const s = await gitService.getSettings();
      settings.value = s;
      isLoaded.value = true;
      
      // Apply theme
      if (s.theme) {
        document.documentElement.setAttribute('data-theme', s.theme);
      }
    } catch (err) {
      console.error("Failed to fetch settings", err);
      throw err;
    }
  };

  const saveSettings = async () => {
    if (!settings.value) return;
    try {
      await gitService.saveSettings(settings.value);
    } catch (err) {
      console.error("Failed to save settings", err);
      throw err;
    }
  };

  const toggleTheme = () => {
    if (settings.value) {
      settings.value.theme = settings.value.theme === 'dark' ? 'light' : 'dark';
      document.documentElement.setAttribute('data-theme', settings.value.theme);
      saveSettings(); // This is async but we don't need to await for UI update
    }
  };

  // Watch for theme changes from external sources
  watch(() => settings.value?.theme, (newTheme) => {
    if (newTheme) {
      document.documentElement.setAttribute('data-theme', newTheme);
    }
  });

  return {
    // State
    settings,
    isLoaded,
    
    // Actions
    fetchSettings,
    saveSettings,
    toggleTheme,
  };
});
