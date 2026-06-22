import { test, expect } from "@playwright/test";

test.describe("Product Gate", () => {
  test.beforeEach(async ({ page }) => {
    await page.addInitScript(() => {
      localStorage.clear();
      localStorage.setItem("i18nextLng", "en");
    });
  });

  test("local mode boots and core product loop stays usable", async ({ page }) => {
    await page.goto("/");

    await page.getByRole("button", { name: "Local Mode" }).click();

    await expect(page.getByText("Connected")).toBeVisible();
    await expect(page.getByRole("heading", { name: "Dashboard" })).toBeVisible();
    await expect(page.getByRole("button", { name: "New Workflow" })).toBeVisible();

    await page.getByRole("button", { name: "Agents" }).click();
    await expect(page.getByRole("heading", { name: "Agent Center" })).toBeVisible();
    await page.getByRole("button", { name: "Register" }).click();
    await page.getByPlaceholder("Agent name......").fill("E2E Agent");
    await page.getByRole("button", { name: "Confirm" }).click();
    await expect(page.getByRole("button", { name: /E2E Agent/ })).toBeVisible();

    await page.getByRole("button", { name: "Knowledge" }).click();
    await expect(page.getByRole("heading", { name: "Knowledge Base" })).toBeVisible();
    await page.getByRole("button", { name: "Upload" }).click();
    await page.getByPlaceholder("Document title...").fill("E2E Note");
    await page.getByPlaceholder("Enter text content to index...").fill("MapleOS product gate knowledge seed.");
    await page.getByRole("button", { name: "Submit Index" }).click();
    await page.getByRole("button", { name: "Recent Index" }).click();
    await expect(page.getByText("E2E Note")).toBeVisible();

    await page.getByRole("button", { name: "Settings" }).click();
    await expect(page.getByRole("heading", { name: "Settings" })).toBeVisible();
    await expect(page.getByText("LLM Models")).toBeVisible();
    await expect(page.getByText("Default Model Route")).toBeVisible();
    await page.getByPlaceholder("http://localhost:11434").fill("http://127.0.0.1:11434");
    await page.getByRole("button", { name: "Save Config" }).click();
    await expect(page.getByText("Saved")).toBeVisible();

    await page.getByRole("button", { name: "Chat" }).click();
    await expect(page.getByRole("heading", { name: "Chat" })).toBeVisible();
    await expect(page.getByRole("button", { name: "Send" })).toBeVisible();
    await expect(page.getByText(/messages/i)).toBeVisible();

    await page.getByRole("button", { name: "Workflows" }).click();
    await expect(page.getByRole("heading", { name: "Workflow Editor" })).toBeVisible();
    await page.getByRole("button", { name: "New" }).click();
    await page.getByPlaceholder("Workflow name...").fill("E2E Flow");
    await page.getByRole("button", { name: "Create" }).click();
    await expect(page.getByText("E2E Flow")).toBeVisible();
    await expect(page.getByText("Node Library")).toBeVisible();
  });
});
