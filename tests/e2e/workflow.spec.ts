import { test, expect } from "./fixtures";

test.describe("Workflow", () => {
  test.beforeEach(async ({ apiMock }) => {
    await apiMock.goto("/");
    await apiMock.locator("nav >> text=工作流").click();
  });

  test("workflow editor loads", async ({ apiMock }) => {
    const header = apiMock.locator("h2 >> text=工作流编辑器");
    await expect(header).toBeVisible({ timeout: 5000 });
  });

  test("node palette visible", async ({ apiMock }) => {
    const palette = apiMock.locator("button").filter({ hasText: /LLM|工具|条件|审批|触发/ });
    await expect(palette.first()).toBeVisible({ timeout: 5000 });
  });

  test("can add node to canvas", async ({ apiMock }) => {
    const llmBtn = apiMock.locator("button >> text=LLM 调用");
    await llmBtn.click();
    const node = apiMock.locator("[data-testid='wf-node'], .absolute.group").first();
    await expect(node).toBeVisible({ timeout: 3000 });
  });
});