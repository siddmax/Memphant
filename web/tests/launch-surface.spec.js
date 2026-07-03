import { expect, test } from "@playwright/test";

const routes = [
  ["/", "MemPhant"],
  ["/docs", "Docs"],
  ["/dashboard", "Developer dashboard"],
  ["/traces", "Trace ret_checkout_001"],
  ["/traces/ret_checkout_001", "Trace ret_checkout_001"],
  ["/memory", "Memory inspector"],
  ["/api-keys", "API keys and usage"],
  ["/evals", "Eval run viewer"],
  ["/exports", "Compiled memory export viewer"],
  ["/citations/cit_token_v2", "Citation cit_token_v2"]
];

test("WS-G routes render their primary content", async ({ page }) => {
  for (const [route, heading] of routes) {
    await page.goto(route);
    await expect(page.getByRole("heading", { name: heading, level: 1 })).toBeVisible();
  }
});

test("trace explorer exposes stage, candidate, context, dropped, and copy affordances", async ({ page }) => {
  await page.goto("/traces/ret_checkout_001");

  await expect(page.getByRole("heading", { name: "Stage timing" })).toBeVisible();
  await expect(page.locator(".stage-list").getByText("context assembly", { exact: true })).toBeVisible();
  const candidateTable = page.getByRole("table", { name: "Candidates grouped by retrieval channel" });
  await expect(candidateTable).toBeVisible();
  await expect(candidateTable.getByText("Callback token v2 is current")).toBeVisible();
  await expect(page.getByRole("button", { name: /Copy trace ID/ })).toBeVisible();
  await expect(page.locator(".trace-layout").getByText("mem_low_trust_blog")).toBeVisible();
  await expect(page.locator(".trace-layout").getByText("scope=project:checkout")).toBeVisible();
});

test("memory and export rows link every visible unit to evidence", async ({ page }) => {
  for (const route of ["/memory", "/exports"]) {
    await page.goto(route);
    const rows = page.locator("[data-memory-unit]");
    await expect(rows.first()).toBeVisible();
    const count = await rows.count();
    expect(count).toBeGreaterThan(0);
    for (let index = 0; index < count; index += 1) {
      const hrefs = await rows.nth(index).locator("a[data-evidence-link]").evaluateAll((links) =>
        links.map((link) => link.getAttribute("href") || "")
      );
      expect(hrefs.some((href) => href.startsWith("/traces/") || href.startsWith("/citations/"))).toBe(true);
    }
  }
});

test("accessibility basics are present on debugging surfaces", async ({ page }) => {
  await page.goto("/traces/ret_checkout_001");

  await expect(page.locator("caption")).toHaveCount(1);
  await expect(page.locator(".badge[aria-label^='status']").first()).toBeVisible();
  await expect(page.getByRole("link", { name: "Skip to content" })).toBeVisible();

  await page.keyboard.press("Tab");
  await expect(page.locator(":focus")).toBeVisible();

  await page.goto("/citations/cit_token_v2");
  await expect(page.getByRole("table", { name: "Citation evidence" })).toBeVisible();
  await expect(page.getByText("sha256:6e0d7b")).toBeVisible();
});

test("public surface fetches API-shaped data and never DB or SQL paths", async ({ page }) => {
  const requests = [];
  page.on("request", (request) => requests.push(new URL(request.url()).pathname));

  await page.goto("/dashboard");
  await expect(page.getByRole("heading", { name: "Developer dashboard" })).toBeVisible();

  expect(requests).toContain("/api/fixture/launch-surface.json");
  expect(requests.some((pathname) => /(?:db|sql|postgres|supabase)/i.test(pathname))).toBe(false);
});

test("benchmark surface avoids unsupported bare SOTA claim", async ({ page }) => {
  await page.goto("/evals");
  await expect(page.getByText("internal_reproduced")).toBeVisible();
  await expect(page.getByText("SOTA", { exact: true })).toHaveCount(0);
});
