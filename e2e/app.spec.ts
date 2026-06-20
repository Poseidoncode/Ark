import { test, expect } from '@playwright/test';

test.describe('Ark Git GUI - E2E Tests', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('應該顯示應用標題', async ({ page }) => {
    await expect(page).toHaveTitle(/Ark/);
  });

  test('應該顯示初始化界面', async ({ page }) => {
    const openButton = page.getByRole('button', { name: /Open Repository/i });
    await expect(openButton).toBeVisible();
    
    const recentSection = page.getByText(/Recent Repositories/i);
    await expect(recentSection).toBeVisible();
  });

  test('應該顯示設置按鈕', async ({ page }) => {
    const settingsButton = page.locator('button[title="Settings"]').first();
    await expect(settingsButton).toBeVisible();
  });

  test('應該能打開設置模態框', async ({ page }) => {
    const settingsButton = page.locator('button[title="Settings"]').first();
    await settingsButton.click();
    
    const settingsModal = page.getByRole('heading', { name: /Settings/i });
    await expect(settingsModal).toBeVisible();
  });

  test('應該能切換設置標籤', async ({ page }) => {
    const settingsButton = page.locator('button[title="Settings"]').first();
    await settingsButton.click();
    
    const gitTab = page.getByRole('button', { name: /Git/i });
    await gitTab.click();
    
    const appearanceTab = page.getByRole('button', { name: /Appearance/i });
    await appearanceTab.click();
    
    await expect(appearanceTab).toHaveAttribute('aria-selected', 'true');
  });
});
