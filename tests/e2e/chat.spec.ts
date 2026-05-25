import { test, expect } from "./fixtures";

test.describe("Chat", () => {
  test.beforeEach(async ({ apiMock }) => {
    await apiMock.goto("/");
    await apiMock.locator("nav >> text=对话").click();
  });

  test("chat panel loads with agent selector", async ({ apiMock }) => {
    const selector = apiMock.locator("select").filter({ has: apiMock.locator("option") });
    await expect(selector).toBeVisible({ timeout: 5000 });
  });

  test("can type message in input", async ({ apiMock }) => {
    const input = apiMock.locator("input[type='text'], textarea").first();
    await input.fill("Hello test message");
    await expect(input).toHaveValue("Hello test message");
  });

  test("shows quick prompt presets", async ({ apiMock }) => {
    const prompts = apiMock.locator("[data-testid='quick-prompt'], button").filter({ hasText: /帮我|分析|解释|推荐/ });
    await expect(prompts.first()).toBeVisible({ timeout: 5000 });
  });
});