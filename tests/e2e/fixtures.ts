import { test as base, expect } from "@playwright/test";

const test = base.extend({
  apiMock: async ({ page }, use) => {
    // Force Chinese locale and local mode for consistent selectors
    await page.addInitScript(() => {
      localStorage.setItem("i18nextLng", "zh");
      localStorage.setItem("mapleos-mode", "local");
      localStorage.setItem("mapleos-auth-token", "test-token");
      localStorage.setItem("mapleos-device-id", "test-device-001");
      localStorage.setItem(
        "mapleos-auth-user",
        JSON.stringify({ user_id: "test-user", username: "tester", role: "user" })
      );
    });
    // Mock device-login to succeed
    await page.route("**/api/maple/api/auth/device-login", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ token: "test-token", user_id: "test-user", username: "tester", role: "user" }),
      });
    });
    await page.route("**/api/maple/api/system/info", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({
          version: "0.1.0",
          uptime_secs: 3600,
          agents_count: 3,
          workflows_count: 5,
          tasks_count: 12,
        }),
      });
    });
    // Mock tasks/stats
    await page.route("**/api/maple/api/tasks/stats", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ total: 12, pending: 2, running: 3, completed: 5, failed: 1, dead_letter: 1 }),
      });
    });
    // Mock agents/status
    await page.route("**/api/maple/api/agents/status", async (route) => {
      await route.fulfill({
        status: 200,
        contentType: "application/json",
        body: JSON.stringify({ agents: [{ id: "agent-1", name: "Code Assistant", status: "Online", is_online: true }], summary: { total: 1, online: 1, offline: 0, busy: 0 } }),
      });
    });
    await page.route("**/rpc", async (route) => {
      const req = route.request();
      const postData = req.postDataJSON();
      if (postData?.method === "workflow.list") {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            jsonrpc: "2.0",
            result: { workflows: [] },
            id: postData.id,
          }),
        });
      } else if (postData?.method === "agent.list") {
        await route.fulfill({
          status: 200,
          contentType: "application/json",
          body: JSON.stringify({
            jsonrpc: "2.0",
            result: {
              agents: [
                { id: "agent-1", name: "Code Assistant" },
                { id: "agent-2", name: "Data Analyst" },
              ],
            },
            id: postData.id,
          }),
        });
      } else {
        await route.continue();
      }
    });
    await use(page);
  },
});

export { test, expect };