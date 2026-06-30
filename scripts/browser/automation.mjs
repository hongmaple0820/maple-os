#!/usr/bin/env node
/**
 * #10: Browser automation script for MapleOS
 *
 * Requires: puppeteer-core or playwright installed globally or locally.
 * Install: npm install puppeteer-core
 *
 * Usage:
 *   node automation.mjs --action navigate --url https://example.com
 *   node automation.mjs --action extract --selector "h1"
 *   node automation.mjs --action screenshot --selector ".content"
 *   node automation.mjs --action click --selector "#button"
 *   node automation.mjs --action scroll
 *   node automation.mjs --action wait --wait-ms 2000
 */

import { parseArgs } from "node:util";

const { values } = parseArgs({
  options: {
    action: { type: "string" },
    url: { type: "string", default: "" },
    selector: { type: "string", default: "" },
    "wait-ms": { type: "string", default: "1000" },
  },
});

async function main() {
  const { action, url, selector } = values;
  const waitMs = parseInt(values["wait-ms"] || "1000", 10);

  let puppeteer;
  try {
    puppeteer = await import("puppeteer-core");
  } catch {
    console.log(JSON.stringify({
      error: "puppeteer-core is not installed. Run: npm install puppeteer-core",
      action,
      mode: "browser",
    }));
    process.exit(1);
  }

  const executablePath = process.env.PUPPETEER_EXECUTABLE_PATH ||
    "/usr/bin/chromium-browser" || "/usr/bin/google-chrome" ||
    "/usr/bin/chromium";

  let browser;
  try {
    browser = await puppeteer.launch({
      executablePath,
      headless: "new",
      args: ["--no-sandbox", "--disable-setuid-sandbox"],
    });
  } catch (e) {
    console.log(JSON.stringify({
      error: `Failed to launch browser: ${e.message}. Set PUPPETEER_EXECUTABLE_PATH to your Chrome/Chromium path.`,
      action,
      mode: "browser",
    }));
    process.exit(1);
  }

  const page = await browser.newPage();
  const result = await executeAction(page, action, url, selector, waitMs);
  await browser.close();
  console.log(JSON.stringify(result));
}

async function executeAction(page, action, url, selector, waitMs) {
  switch (action) {
    case "navigate": {
      if (!url) return { error: "url is required for navigate" };
      const response = await page.goto(url, { waitUntil: "domcontentloaded", timeout: 15000 });
      const title = await page.title();
      const text = await page.evaluate(() => document.body.innerText.substring(0, 2000));
      return {
        action: "navigate",
        url,
        status: response?.status() || 0,
        title,
        text: text + (text.length >= 2000 ? "...[truncated]" : ""),
        mode: "browser",
      };
    }
    case "click": {
      if (!selector) return { error: "selector is required for click" };
      await page.click(selector);
      await page.waitForTimeout(waitMs);
      return { action: "click", selector, mode: "browser" };
    }
    case "extract": {
      if (!selector) return { error: "selector is required for extract" };
      const text = await page.$eval(selector, (el) => el.textContent?.trim() || "");
      return { action: "extract", selector, text, mode: "browser" };
    }
    case "screenshot": {
      const buffer = await page.screenshot({ encoding: "base64" });
      return { action: "screenshot", base64: buffer, mode: "browser" };
    }
    case "scroll": {
      await page.evaluate(() => window.scrollBy(0, window.innerHeight));
      await page.waitForTimeout(waitMs);
      return { action: "scroll", mode: "browser" };
    }
    case "wait": {
      await page.waitForTimeout(waitMs);
      return { action: "wait", wait_ms: waitMs, mode: "browser" };
    }
    default:
      return { error: `unknown action: ${action}` };
  }
}

main().catch((e) => {
  console.log(JSON.stringify({ error: e.message, mode: "browser" }));
  process.exit(1);
});
