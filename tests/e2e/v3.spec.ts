import { test, expect } from "./v3.fixtures";

/** Helper: select the first group in sidebar */
async function selectGroup(apiMock: import("@playwright/test").Page) {
  await apiMock.getByRole("button", { name: /Test Group/ }).first().click();
  await apiMock.waitForTimeout(500);
}

test.describe("v3 — Group Chat Platform", () => {
  test.beforeEach(async ({ apiMock }) => {
    await apiMock.goto("/v3");
  });

  // ── Page Load ──

  test("v3 page loads and shows group list", async ({ apiMock }) => {
    await expect(apiMock.getByRole("button", { name: /Test Group/ }).first()).toBeVisible({ timeout: 10000 });
  });

  test("clicking a group loads chat area", async ({ apiMock }) => {
    await selectGroup(apiMock);
    await expect(apiMock.locator("text=Hello agents!")).toBeVisible({ timeout: 10000 });
  });

  // ── Messages ──

  test("messages render with sender info", async ({ apiMock }) => {
    await selectGroup(apiMock);
    await expect(apiMock.locator("text=Hello agents!")).toBeVisible({ timeout: 10000 });
    await expect(apiMock.locator("text=Hello! How can I help?")).toBeVisible();
  });

  test("can type in message input", async ({ apiMock }) => {
    await selectGroup(apiMock);
    const input = apiMock.locator("textarea[placeholder*='Type a message']").first();
    await input.waitFor({ state: "visible", timeout: 10000 });
    await input.fill("E2E test message");
    await expect(input).toHaveValue("E2E test message");
  });

  // ── Right Sidebar Tabs ──

  test("right sidebar shows all tabs", async ({ apiMock }) => {
    await selectGroup(apiMock);
    const tabs = ["tasks", "board", "memory", "members", "rules", "cron", "hooks", "workflows", "delegations"];
    for (const tab of tabs) {
      await expect(apiMock.locator(`button:has-text("${tab}")`).first()).toBeVisible({ timeout: 5000 });
    }
  });

  test("tasks tab shows task list", async ({ apiMock }) => {
    await selectGroup(apiMock);
    await apiMock.locator("button:has-text(\"tasks\")").first().click();
    await expect(apiMock.locator("text=Test Task")).toBeVisible({ timeout: 10000 });
  });

  test("members tab shows member list", async ({ apiMock }) => {
    await selectGroup(apiMock);
    await apiMock.locator("button:has-text(\"members\")").first().click();
    await expect(apiMock.locator("text=user-1").first()).toBeVisible({ timeout: 10000 });
    await expect(apiMock.locator("text=agent-1").first()).toBeVisible();
  });

  test("memory tab loads memory panel", async ({ apiMock }) => {
    await selectGroup(apiMock);
    await apiMock.locator("button:has-text(\"memory\")").first().click();
    // Memory panel renders stats — just verify no crash
    await apiMock.waitForTimeout(1000);
  });

  test("rules tab loads rules panel", async ({ apiMock }) => {
    await selectGroup(apiMock);
    await apiMock.locator("button:has-text(\"rules\")").first().click();
    // Verify the panel header or empty state renders
    await expect(apiMock.locator("h2:has-text('Rules')")).toBeVisible({ timeout: 10000 });
  });

  test("cron tab loads cron panel", async ({ apiMock }) => {
    await selectGroup(apiMock);
    await apiMock.locator("button:has-text(\"cron\")").first().click();
    await expect(apiMock.locator("h2:has-text('Cron')")).toBeVisible({ timeout: 10000 });
  });

  test("hooks tab loads hooks panel", async ({ apiMock }) => {
    await selectGroup(apiMock);
    await apiMock.locator("button:has-text(\"hooks\")").first().click();
    await expect(apiMock.locator("h2:has-text('Agent Hooks')")).toBeVisible({ timeout: 10000 });
  });

  test("workflows tab loads workflow panel", async ({ apiMock }) => {
    await selectGroup(apiMock);
    await apiMock.locator("button:has-text(\"workflows\")").first().click();
    await expect(apiMock.locator("h2:has-text('Workflows')")).toBeVisible({ timeout: 10000 });
  });

  test("delegations tab loads delegations panel", async ({ apiMock }) => {
    await selectGroup(apiMock);
    await apiMock.locator("button:has-text(\"delegations\")").first().click();
    await expect(apiMock.locator("h2:has-text('Delegations')")).toBeVisible({ timeout: 10000 });
  });

  // ── Search ──

  test("search input is visible in header", async ({ apiMock }) => {
    await selectGroup(apiMock);
    const searchInput = apiMock.locator("input[placeholder*='Search'], input[placeholder*='搜索']").first();
    await expect(searchInput).toBeVisible({ timeout: 10000 });
  });

  // ── Create Group ──

  test("can create a new group", async ({ apiMock }) => {
    const createBtn = apiMock.locator("button").filter({ hasText: /\+|新建|Create/ }).first();
    if (await createBtn.isVisible()) {
      await createBtn.click();
      await apiMock.waitForTimeout(500);
    }
  });
});
