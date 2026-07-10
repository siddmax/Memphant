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

test("quickstart surfaces only real commands", async ({ page }) => {
  for (const route of ["/", "/docs"]) {
    await page.goto(route);
    const shell = page.locator("#content");
    await expect(shell.locator("pre").first()).toContainText("docker compose up -d");
    await expect(shell.locator("pre").first()).toContainText("apply_memphant_migrations.py --database-url $DATABASE_URL");
    await expect(shell.locator("pre").first()).toContainText("/v1/recall");
    await expect(shell.locator("pre").first()).toContainText("Authorization: Bearer $MEMPHANT_API_KEY");
    await expect(shell.getByText("CLI verbs land with the runtime kernel")).toBeVisible();
    await expect(shell.getByText("cargo install memphant-cli")).toHaveCount(0);
    await expect(shell.getByText("./incident.md")).toHaveCount(0);
  }
  await page.goto("/docs");
  await expect(page.locator("#content pre").first()).toContainText("memphant db lint --provider plain-postgres");
});

test("fixture-rendered pages carry a visible demo-data badge", async ({ page }) => {
  for (const route of ["/dashboard", "/memory"]) {
    await page.goto(route);
    await expect(page.locator("[data-demo-badge]")).toBeVisible();
    await expect(page.locator("[data-demo-badge]")).toHaveText("Demo data");
  }
});

test("correct and forget render disabled, not missing, without an API base", async ({ page }) => {
  await page.goto("/memory");
  const correct = page.getByRole("button", { name: /^Correct / }).first();
  const forget = page.getByRole("button", { name: /^Forget / }).first();
  await expect(correct).toBeVisible();
  await expect(forget).toBeVisible();
  await expect(correct).toBeDisabled();
  await expect(forget).toBeDisabled();
  await expect(correct).toHaveAttribute("title", "Connect an API base to enable");
  await expect(forget).toHaveAttribute("title", "Connect an API base to enable");
});

test("correct and forget fire real API calls when an API base is configured", async ({ page }) => {
  await page.addInitScript(() => {
    window.MEMPHANT_API_BASE = "https://api.memphant.test";
    sessionStorage.setItem("memphant-api-key", "test-key");
  });
  const apiCalls = [];
  await page.route("https://api.memphant.test/**", async (route) => {
    const request = route.request();
    apiCalls.push({
      path: new URL(request.url()).pathname,
      auth: request.headers()["authorization"]
    });
    await route.fulfill({ status: 200, contentType: "application/json", body: "{}" });
  });

  await page.goto("/memory");
  const forget = page.getByRole("button", { name: /^Forget / }).first();
  await expect(forget).toBeEnabled();
  await forget.click();

  await expect(page.locator("[data-action-feedback]").first()).toHaveText("forget accepted");
  await expect(page.locator("[data-unit-state]").first()).toContainText("deleted");
  expect(apiCalls).toEqual([{ path: "/v1/forget", auth: "Bearer test-key" }]);
});

test("copy trace ID failure shows inline error instead of throwing", async ({ page }) => {
  await page.addInitScript(() => {
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: () => Promise.reject(new Error("NotAllowedError")) }
    });
  });
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(error));

  await page.goto("/traces/ret_checkout_001");
  await page.getByRole("button", { name: /Copy trace ID/ }).click();

  await expect(page.locator("[data-copy-error]")).toBeVisible();
  await expect(page.locator("[data-copy-error]")).toContainText("copy failed");
  expect(pageErrors).toEqual([]);
});

test("no route overflows a 390px viewport", async ({ page }, testInfo) => {
  test.skip(testInfo.project.name !== "mobile", "mobile project only");
  for (const [route] of routes) {
    await page.goto(route);
    await expect(page.locator("h1")).toBeVisible();
    const scrollWidth = await page.evaluate(() => document.documentElement.scrollWidth);
    expect(scrollWidth, `route ${route} overflows horizontally`).toBeLessThanOrEqual(390);
  }
});

test("benchmark surface avoids unsupported bare SOTA claim", async ({ page }) => {
  await page.goto("/evals");
  await expect(page.getByText("internal_reproduced")).toBeVisible();
  await expect(page.getByText("SOTA", { exact: true })).toHaveCount(0);
});
