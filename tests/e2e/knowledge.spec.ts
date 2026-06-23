import { test, expect } from "./fixtures";

test.describe("Knowledge", () => {
  test.beforeEach(async ({ apiMock }) => {
    await apiMock.goto("/");
    await apiMock.locator("nav >> text=知识库").click();
  });

  test("knowledge manager loads", async ({ apiMock }) => {
    const header = apiMock.locator("h2 >> text=知识库");
    await expect(header).toBeVisible({ timeout: 5000 });
  });

  test("search input visible", async ({ apiMock }) => {
    const searchInput = apiMock.locator("input[placeholder*='搜索']");
    await expect(searchInput).toBeVisible({ timeout: 5000 });
  });

  test("index upload form visible", async ({ apiMock }) => {
    await apiMock.getByRole("button", { name: "上传" }).click();
    const upload = apiMock.locator("textarea");
    await expect(upload.first()).toBeVisible({ timeout: 5000 });
  });
});