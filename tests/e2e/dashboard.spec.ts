import { test, expect } from "./fixtures";

test.describe("Dashboard", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("page loads successfully", async ({ page }) => {
    await expect(page).toHaveTitle(/MapleOS/);
  });

  test("shows system info metrics", async ({ apiMock }) => {
    await apiMock.goto("/");
    const metrics = apiMock.locator("[data-testid='metric-card'], .text-muted-foreground");
    await expect(metrics.first()).toBeVisible({ timeout: 5000 });
  });

  test("quick action cards visible", async ({ apiMock }) => {
    await apiMock.goto("/");
    const actions = apiMock.locator("button, a").filter({ hasText: /新建|运行|搜索/ });
    await expect(actions.first()).toBeVisible({ timeout: 5000 });
  });
});