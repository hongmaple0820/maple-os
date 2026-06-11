import { test as base, expect } from "@playwright/test";

/** Mock data */
const mockGroup = {
  id: "grp-001",
  name: "Test Group",
  description: "E2E test group",
  group_type: "collaboration",
  owner_id: "user-1",
  settings: { max_agents: 10, auto_approve: false, knowledge_base_enabled: true, allow_member_invite: true },
  member_count: 3,
  message_count: 0,
  created_at: Math.floor(Date.now() / 1000),
  updated_at: Math.floor(Date.now() / 1000),
};

const mockMembers = [
  { group_id: "grp-001", member_id: "user-1", member_type: "human", role: "owner", can_approve: true, joined_at: Math.floor(Date.now() / 1000) },
  { group_id: "grp-001", member_id: "agent-1", member_type: "agent", role: "member", can_approve: true, joined_at: Math.floor(Date.now() / 1000) },
  { group_id: "grp-001", member_id: "agent-2", member_type: "agent", role: "member", can_approve: false, joined_at: Math.floor(Date.now() / 1000) },
];

const mockMessages = [
  { id: "msg-001", group_id: "grp-001", sender_id: "user-1", sender_type: "human", message_type: "text", content: "Hello agents!", thread_reply_count: 0, source_channel: "web", pinned: false, created_at: Math.floor(Date.now() / 1000) },
  { id: "msg-002", group_id: "grp-001", sender_id: "agent-1", sender_type: "agent", message_type: "text", content: "Hello! How can I help?", thread_reply_count: 2, source_channel: "web", pinned: false, created_at: Math.floor(Date.now() / 1000) },
];

const test = base.extend<{ apiMock: import("@playwright/test").Page }>({
  apiMock: async ({ page }, use) => {
    await page.addInitScript(() => {
      localStorage.setItem("mapleos-v3-user-id", "user-1");
      localStorage.setItem("mapleos-v3-token", "test-v3-token");
    });

    // ── Auth ──
    await page.route("**/api/v3/auth/**", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ token: "test-v3-token", user_id: "user-1" }) });
    });

    // ── Groups ──
    await page.route("**/api/v3/groups", async (route) => {
      if (route.request().method() === "GET") {
        await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ groups: [mockGroup] }) });
      } else if (route.request().method() === "POST") {
        const body = route.request().postDataJSON();
        await route.fulfill({
          status: 200, contentType: "application/json",
          body: JSON.stringify({ group: { ...mockGroup, id: "grp-new", name: body.name || "New Group" } }),
        });
      } else {
        await route.continue();
      }
    });

    // ── Members ──
    await page.route("**/api/v3/groups/*/members", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ members: mockMembers }) });
    });

    // ── Messages ──
    await page.route("**/api/v3/groups/*/messages?**", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ messages: mockMessages, has_more: false }) });
    });
    await page.route("**/api/v3/groups/*/messages", async (route) => {
      if (route.request().method() === "GET") {
        await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ messages: mockMessages, has_more: false }) });
      } else if (route.request().method() === "POST") {
        const body = route.request().postDataJSON();
        await route.fulfill({
          status: 200, contentType: "application/json",
          body: JSON.stringify({ message: { id: "msg-new", group_id: "grp-001", sender_id: "user-1", sender_type: "human", message_type: "text", content: body.content, thread_reply_count: 0, source_channel: "web", pinned: false, created_at: Math.floor(Date.now() / 1000) } }),
        });
      } else {
        await route.continue();
      }
    });

    // ── Tasks ──
    await page.route("**/api/v3/tasks?**", async (route) => {
      await route.fulfill({
        status: 200, contentType: "application/json",
        body: JSON.stringify({ tasks: [
          { id: "task-1", title: "Test Task", status: "todo", priority: "high", creator_id: "user-1", created_at: Math.floor(Date.now() / 1000), updated_at: Math.floor(Date.now() / 1000) },
          { id: "task-2", title: "Another Task", status: "in_progress", priority: "medium", creator_id: "agent-1", created_at: Math.floor(Date.now() / 1000), updated_at: Math.floor(Date.now() / 1000) },
        ] }),
      });
    });

    // ── Approvals ──
    await page.route("**/api/v3/approvals/**", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ approvals: [] }) });
    });

    // ── Memory ──
    await page.route("**/api/v3/memories/**", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ stats: { agent_id: "default", working_count: 5, episodic_count: 10, semantic_count: 20, total_count: 35 }, results: [] }) });
    });

    // ── DMs ──
    await page.route("**/api/v3/dms", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ dms: [] }) });
    });

    // ── Rules ──
    await page.route("**/api/v3/groups/*/rules", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ rules: [] }) });
    });

    // ── Cron ──
    await page.route("**/api/v3/groups/*/cron", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ jobs: [] }) });
    });

    // ── Hooks ──
    await page.route("**/api/v3/groups/*/hooks", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ hooks: [] }) });
    });

    // ── Workflows ──
    await page.route("**/api/v3/workflows", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ workflows: [] }) });
    });
    await page.route("**/api/v3/workflow-runs**", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ runs: [] }) });
    });

    // ── Threads ──
    await page.route("**/api/v3/groups/*/messages/*/thread", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ messages: [] }) });
    });

    // ── Search ──
    await page.route("**/api/v3/groups/*/messages/search**", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ messages: [] }) });
    });

    // ── Delegations ──
    await page.route("**/api/v3/dms/*/delegations", async (route) => {
      await route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify({ delegations: [] }) });
    });

    await use(page);
  },
});

export { test, expect };
