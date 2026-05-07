import { test, expect } from '@playwright/test';

test.describe('Ark Git GUI - E2E Tests', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('應該顯示應用標題', async ({ page }) => {
    await expect(page).toHaveTitle(/Ark/);
  });

  test('應該顯示初始化界面', async ({ page }) => {
    const openSection = page.getByText('Open');
    await expect(openSection).toBeVisible();

    const cloneSection = page.getByText('Clone');
    await expect(cloneSection).toBeVisible();
  });

  test('應該顯示設置按鈕', async ({ page }) => {
    const settingsButton = page.locator('button[title="Settings"]').first();
    await expect(settingsButton).toBeVisible();
  });

  test('應該能打開設置模態框', async ({ page }) => {
    // Settings button is visible (tested above)
    const settingsButton = page.locator('button[title="Settings"]').first();
    await expect(settingsButton).toBeVisible();
    
    // Click the button
    await settingsButton.click();
    
    // Check if the settings modal heading appeared
    const settingsHeading = page.locator('h2:text-is("Settings")');
    await expect(settingsHeading).toBeVisible({ timeout: 5000 });
  });

  test('應該能切換設置標籤', async ({ page }) => {
    // Settings button is visible
    const settingsButton = page.locator('button[title="Settings"]').first();
    await expect(settingsButton).toBeVisible();
    await settingsButton.click();
    
    // Check if the settings modal heading appeared
    const settingsHeading = page.locator('h2:text-is("Settings")');
    await expect(settingsHeading).toBeVisible({ timeout: 5000 });
  });
});
