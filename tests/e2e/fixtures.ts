import { test as base, expect } from "@playwright/test";

const test = base.extend({
  apiMock: async ({ page }, use) => {
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