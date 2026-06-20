import { test, expect } from '@playwright/test';

test.describe('Ark Git GUI - E2E Tests', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should display application title', async ({ page }) => {
    await expect(page).toHaveTitle(/Ark/);
  });

  test('should show welcome screen with Open and Clone buttons', async ({ page }) => {
    await expect(page.getByText('Open')).toBeVisible();
    await expect(page.getByText('Clone')).toBeVisible();
    await expect(page.getByText('A precision Git client for professional workflows')).toBeVisible();
  });

  test('should display Settings and Theme toggle buttons', async ({ page }) => {
    await expect(page.locator('button[title="Settings"]').first()).toBeVisible();
    await expect(page.locator('button[title="Dark Mode"]').first()).toBeVisible();
  });

  test('should open and close Settings modal', async ({ page }) => {
    await page.locator('button[title="Settings"]').first().click();
    await expect(page.locator('h2:text-is("Settings")')).toBeVisible({ timeout: 5000 });
    await page.getByText('Cancel').first().click();
    await expect(page.locator('h2:text-is("Settings")')).not.toBeVisible();
  });

  test('should toggle theme between dark and light', async ({ page }) => {
    const themeBtn = page.locator('button[title="Dark Mode"]').first();
    await expect(themeBtn).toBeVisible();
    await themeBtn.click();
    await expect(page.locator('button[title="Light Mode"]').first()).toBeVisible({ timeout: 5000 });
  });

  test('should open Clone modal with URL and path fields', async ({ page }) => {
    await page.getByText('Clone').first().click();
    await expect(page.locator('h2:text-is("Clone Repository")')).toBeVisible({ timeout: 5000 });
    await expect(page.getByPlaceholder('https://github.com/user/repo.git')).toBeVisible();
    await expect(page.getByPlaceholder('/path/to/destination')).toBeVisible();
    await expect(page.getByText('Clone').last()).toBeDisabled();
  });

  test('should show error toast for unhandled promise rejection', async ({ page }) => {
    await page.evaluate(() => {
      new Promise((_, reject) => reject(new Error('Simulated rejection')));
    });
    await expect(page.getByText('Promise Rejected').first()).toBeVisible({ timeout: 5000 });
  });

  test('should recover from navigation error gracefully', async ({ page }) => {
    await page.goto('/nonexistent');
    await expect(page.locator('h1')).toBeVisible({ timeout: 5000 });
  });

  test.describe('Component isolation', () => {
    test('ContextMenu should not be visible by default', async ({ page }) => {
      const menu = page.locator('[role="menu"]');
      await expect(menu).toHaveCount(0);
    });
  });
});
