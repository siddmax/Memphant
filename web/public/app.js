const content = document.querySelector("#content");
const navLinks = [...document.querySelectorAll(".top-nav a")];

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function statusClass(value) {
  return String(value || "trusted").toLowerCase().replaceAll("_", "-").replaceAll(" ", "-");
}

function badge(label, tone = label) {
  const text = escapeHtml(label);
  return `<span class="badge ${statusClass(tone)}" aria-label="status ${text}"><span aria-hidden="true">●</span>${text}</span>`;
}

function traceLink(id, label = id) {
  return `<a data-evidence-link href="/traces/${encodeURIComponent(id)}">${escapeHtml(label)}</a>`;
}

function citationLink(id, label = id) {
  return `<a data-evidence-link href="/citations/${encodeURIComponent(id)}">${escapeHtml(label)}</a>`;
}

function page(title, eyebrow, lede, body) {
  content.innerHTML = `
    <section class="page-head">
      <p class="eyebrow">${escapeHtml(eyebrow)}</p>
      <h1>${escapeHtml(title)}</h1>
      <p class="lede">${escapeHtml(lede)}</p>
    </section>
    ${body}
  `;
  document.title = `${title} | MemPhant`;
  navLinks.forEach((link) => {
    const current = link.pathname === window.location.pathname || window.location.pathname.startsWith(`${link.pathname}/`);
    if (current && link.pathname !== "/") {
      link.setAttribute("aria-current", "page");
    } else {
      link.removeAttribute("aria-current");
    }
  });
}

function metric(label, value, detail = "") {
  return `<div class="panel metric"><span class="muted">${escapeHtml(label)}</span><strong>${escapeHtml(value)}</strong><span>${escapeHtml(detail)}</span></div>`;
}

function renderHome(data) {
  page(
    "MemPhant",
    "agent memory substrate",
    "A public memory layer for recall, correction, forget, traces, and eval proof.",
    `
      <section class="grid two">
        <div class="panel">
          <h2>Install</h2>
          <pre><code>cargo install memphant-cli
memphant recall --scope project:checkout "Which token is current?"</code></pre>
        </div>
        <div class="panel">
          <h2>Proof block</h2>
          <ul class="pill-list">
            <li class="pill"><span>Trace</span>${traceLink(data.proof.traceId)}</li>
            <li class="pill"><span>Eval run</span><a href="/evals">${escapeHtml(data.proof.evalRunId)}</a></li>
            <li class="pill"><span>Security</span>${badge(data.proof.securitySuite, "trusted")}</li>
          </ul>
        </div>
      </section>
      <section class="band route-grid" aria-label="Launch surface routes">
        ${[
          ["Docs", "/docs", "Quickstart, MCP, SDKs, security, evals"],
          ["Dashboard", "/dashboard", "Keys, usage, traces, recent writes"],
          ["Trace explorer", "/traces", "Candidates, drops, policies, citations"],
          ["Memory inspector", "/memory", "Facts, episodes, resources, deletion state"],
          ["Compiled exports", "/exports", "Read-only Markdown export verification"]
        ].map(([title, href, desc]) => `
          <a class="panel" href="${href}">
            <h2>${title}</h2>
            <p>${desc}</p>
          </a>
        `).join("")}
      </section>
    `
  );
}

function renderDocs() {
  page(
    "Docs",
    "public contract",
    "Start with one public contract: REST, SDK, CLI, and MCP call the same operations.",
    `
      <section class="grid two">
        <div class="panel">
          <h2>Quickstart</h2>
          <pre><code>memphant db lint --provider plain-postgres
memphant retain --scope project:checkout ./incident.md
memphant recall --scope project:checkout "What changed?"</code></pre>
        </div>
        <div class="panel">
          <h2>Core operations</h2>
          <table>
            <caption>Public API verbs</caption>
            <tbody>
              ${["retain", "recall", "reflect", "correct", "forget", "trace", "mark"].map((verb) => `
                <tr><th scope="row">${verb}</th><td>Stable REST, SDK, CLI, and MCP contract.</td></tr>
              `).join("")}
            </tbody>
          </table>
        </div>
      </section>
      <section class="band grid three">
        ${["MCP", "Python", "TypeScript", "Rust server", "Security", "Evals"].map((item) => `
          <article class="panel"><h2>${item}</h2><p>Launch docs are i18n-ready and keep memory content opaque user data.</p></article>
        `).join("")}
      </section>
    `
  );
}

function renderDashboard(data) {
  const trace = data.traces[0];
  page(
    "Developer dashboard",
    "operational overview",
    "API keys, tenants, recent recalls, writes, error rate, trace links, and quota without chart clutter.",
    `
      <section class="grid three">
        ${metric("Recall usage", data.usage.used, `quota ${data.usage.quota}`)}
        ${metric("Error rate", data.usage.errorRate, "last 24h")}
        ${metric("Recall p95", `${data.usage.p95LatencyMs}ms`, `${data.usage.costMicros} cost micros`)}
      </section>
      <section class="band grid two">
        <div class="panel">
          <h2>Recent recalls</h2>
          <table>
            <caption>Recent trace links</caption>
            <thead><tr><th>Trace</th><th>Scope</th><th>Actor</th><th>Latency</th></tr></thead>
            <tbody><tr><td>${traceLink(trace.id)}</td><td>${escapeHtml(trace.scope)}</td><td>${escapeHtml(trace.actor)}</td><td>${trace.latencyMs}ms</td></tr></tbody>
          </table>
        </div>
        <div class="panel">
          <h2>API keys</h2>
          <table>
            <caption>Key status and scopes</caption>
            <tbody>${data.apiKeys.map((key) => `
              <tr><th scope="row">${escapeHtml(key.label)}</th><td><span class="mono">${escapeHtml(key.id)}</span></td><td>${escapeHtml(key.scopes.join(", "))}</td><td>${badge(key.status, key.status === "active" ? "trusted" : "degraded")}</td></tr>
            `).join("")}</tbody>
          </table>
        </div>
      </section>
    `
  );
}

function renderTrace(data, selectedId = data.traces[0].id) {
  const trace = data.traces.find((item) => item.id === selectedId) || data.traces[0];
  page(
    `Trace ${trace.id}`,
    "trace explorer",
    trace.query,
    `
      <section class="panel">
        <div class="actions">
          <span>Scope <strong>${escapeHtml(trace.scope)}</strong></span>
          <span>Actor <strong>${escapeHtml(trace.actor)}</strong></span>
          <span>Latency <strong>${trace.latencyMs}ms</strong></span>
          <span>Degraded <strong>${trace.degraded ? "yes" : "no"}</strong></span>
          <button type="button" data-copy="${escapeHtml(trace.id)}" aria-label="Copy trace ID ${escapeHtml(trace.id)}">Copy trace ID</button>
        </div>
      </section>
      <section class="band trace-layout">
        <aside class="panel" aria-labelledby="stage-heading">
          <h2 id="stage-heading">Stage timing</h2>
          <ol class="stage-list">
            ${trace.stages.map((stage) => `<li class="stage-row"><span>${escapeHtml(stage.name)}</span><strong>${stage.latencyMs}ms</strong></li>`).join("")}
          </ol>
        </aside>
        <section class="panel">
          <h2>Candidates</h2>
          <table>
            <caption>Candidates grouped by retrieval channel</caption>
            <thead><tr><th>Kind</th><th>Summary</th><th>Channel</th><th>Ranks</th><th>Trust</th><th>Citation</th></tr></thead>
            <tbody>
              ${trace.candidates.map((candidate) => `
                <tr data-memory-unit="${escapeHtml(candidate.unitId)}">
                  <td>${escapeHtml(candidate.kind)}</td>
                  <td>${escapeHtml(candidate.summary)} ${candidate.discardReason ? badge(candidate.discardReason, "stale") : ""}</td>
                  <td>${escapeHtml(candidate.channel)}</td>
                  <td>ch ${candidate.channelRank} / fus ${candidate.fusionRank ?? "-"}</td>
                  <td>${badge(candidate.trust)}</td>
                  <td>${citationLink(candidate.citationId)}</td>
                </tr>
              `).join("")}
            </tbody>
          </table>
        </section>
        <aside class="panel">
          <h2>Context pack</h2>
          <ul class="pill-list">
            ${trace.context.map((unitId) => {
              const unit = data.memory.find((item) => item.id === unitId);
              return `<li class="pill" data-memory-unit="${escapeHtml(unitId)}"><span>${escapeHtml(unit?.title || unitId)}</span>${traceLink(trace.id, "trace")}${citationLink(unit?.citationId || "", "citation")}</li>`;
            }).join("")}
          </ul>
          <h2>Dropped</h2>
          <ul class="pill-list">${trace.dropped.map((item) => `<li class="pill"><span>${escapeHtml(item.unitId)}</span>${badge(item.reason, item.reason === "trust" ? "low-trust" : "stale")}</li>`).join("")}</ul>
          <h2>Policy filters</h2>
          <p>${escapeHtml(trace.policyFilters.join(" · "))}</p>
        </aside>
      </section>
      <section class="panel">
        <h2>Raw JSON</h2>
        <pre><code>${escapeHtml(JSON.stringify(trace, null, 2))}</code></pre>
      </section>
    `
  );
}

function renderMemory(data) {
  page(
    "Memory inspector",
    "evidence ledger",
    "Inspect active, superseded, provisional, and deleted states with correction and forget affordances.",
    `
      <section class="panel">
        <table>
          <caption>Memory units with evidence paths</caption>
          <thead><tr><th>Unit</th><th>Kind</th><th>Scope</th><th>Status</th><th>Evidence</th><th>Actions</th></tr></thead>
          <tbody>${data.memory.map((unit) => `
            <tr data-memory-unit="${escapeHtml(unit.id)}">
              <th scope="row">${escapeHtml(unit.title)}<br><span class="muted">${escapeHtml(unit.body)}</span></th>
              <td>${escapeHtml(unit.kind)}</td>
              <td>${escapeHtml(unit.scope)}</td>
              <td>${badge(unit.state, unit.trust)}</td>
              <td>${traceLink(unit.traceId)} ${citationLink(unit.citationId)}</td>
              <td><button type="button" aria-label="Correct ${escapeHtml(unit.id)}">Correct</button> <button type="button" aria-label="Forget ${escapeHtml(unit.id)}">Forget</button></td>
            </tr>
          `).join("")}</tbody>
        </table>
      </section>
    `
  );
}

function renderApiKeys(data) {
  page(
    "API keys and usage",
    "hosted dashboard",
    "Scoped service credentials, usage, and rotation status for the hosted surface.",
    `
      <section class="grid two">
        <div class="panel">
          <h2>Keys</h2>
          <table>
            <caption>Scoped API keys</caption>
            <tbody>${data.apiKeys.map((key) => `<tr><th scope="row">${escapeHtml(key.label)}</th><td class="mono">${escapeHtml(key.id)}</td><td>${escapeHtml(key.scopes.join(", "))}</td><td>${badge(key.status, key.status === "active" ? "trusted" : "degraded")}</td></tr>`).join("")}</tbody>
          </table>
        </div>
        <div class="panel">
          <h2>Usage</h2>
          <ul class="pill-list">
            <li class="pill"><span>Used</span><strong>${escapeHtml(data.usage.used)}</strong></li>
            <li class="pill"><span>Quota</span><strong>${escapeHtml(data.usage.quota)}</strong></li>
            <li class="pill"><span>Error rate</span><strong>${escapeHtml(data.usage.errorRate)}</strong></li>
          </ul>
        </div>
      </section>
    `
  );
}

function renderEvals(data) {
  page(
    "Eval run viewer",
    "quality facts",
    "Accuracy is shown with CI, latency, cost, trace archive, caveats, and source status.",
    `
      <section class="panel">
        <table>
          <caption>Release eval runs</caption>
          <thead><tr><th>Run</th><th>Benchmark</th><th>Accuracy</th><th>Latency</th><th>Cost</th><th>Source</th><th>Trace archive</th><th>Security</th></tr></thead>
          <tbody>${data.evalRuns.map((run) => `<tr><th scope="row">${escapeHtml(run.id)}</th><td>${escapeHtml(run.benchmark)} ${escapeHtml(run.version)}</td><td>${escapeHtml(run.accuracy)} (${escapeHtml(run.ci)})</td><td>${run.latencyP95Ms}ms p95</td><td>${run.costMicros} micros</td><td>${badge(run.sourceStatus, "trusted")}</td><td><span class="mono">${escapeHtml(run.traceArchive)}</span></td><td>${escapeHtml(run.security)}</td></tr>`).join("")}</tbody>
        </table>
      </section>
    `
  );
}

function renderExports(data) {
  page(
    "Compiled memory export viewer",
    "read-only export",
    "Compiled Markdown exports are inspection niceties with lock verification, not a second source of truth.",
    `
      <section class="panel">
        <table>
          <caption>Read-only export entries</caption>
          <thead><tr><th>Export</th><th>Scope</th><th>Status</th><th>Entry</th><th>Evidence</th></tr></thead>
          <tbody>${data.exports.flatMap((entry) => entry.entries.map((item) => `<tr data-memory-unit="${escapeHtml(item.memoryId)}"><th scope="row">${escapeHtml(entry.id)}</th><td>${escapeHtml(entry.scope)}</td><td>${badge(entry.status, "trusted")}</td><td>${escapeHtml(item.title)}</td><td>${traceLink(item.traceId)} ${citationLink(data.memory.find((unit) => unit.id === item.memoryId)?.citationId || "")}</td></tr>`)).join("")}</tbody>
        </table>
      </section>
    `
  );
}

function renderCitation(data, citationId) {
  const citation = data.citations.find((item) => item.id === citationId) || data.citations[0];
  const unit = data.memory.find((item) => item.id === citation.unitId);
  page(
    `Citation ${citation.id}`,
    "citation drawer",
    "Evidence path from memory unit to source episode/resource and retrieval trace.",
    `
      <section class="panel drawer" data-citation="${escapeHtml(citation.id)}">
        <h2>${escapeHtml(unit?.title || citation.unitId)}</h2>
        <table>
          <caption>Citation evidence</caption>
          <tbody>
            <tr><th scope="row">Memory unit</th><td>${escapeHtml(citation.unitId)}</td></tr>
            <tr><th scope="row">Episode</th><td>${escapeHtml(citation.episodeId)}</td></tr>
            <tr><th scope="row">Resource</th><td>${escapeHtml(citation.resourceId)}</td></tr>
            <tr><th scope="row">Trust</th><td>${badge(citation.trust, "trusted")}</td></tr>
            <tr><th scope="row">Validity</th><td>${escapeHtml(citation.validFrom)}${citation.validTo ? ` to ${escapeHtml(citation.validTo)}` : ""}</td></tr>
            <tr><th scope="row">Quote hash</th><td class="mono">${escapeHtml(citation.quoteHash)}</td></tr>
            <tr><th scope="row">Trace</th><td>${traceLink(citation.traceId)}</td></tr>
          </tbody>
        </table>
      </section>
    `
  );
}

function wireCopyButtons() {
  document.querySelectorAll("[data-copy]").forEach((button) => {
    button.addEventListener("click", async () => {
      const value = button.getAttribute("data-copy") || "";
      await navigator.clipboard?.writeText(value);
      button.textContent = "Copied";
    });
  });
}

function route(data) {
  const parts = window.location.pathname.split("/").filter(Boolean);
  const top = parts[0] || "";
  if (top === "docs") renderDocs(data);
  else if (top === "dashboard") renderDashboard(data);
  else if (top === "traces") renderTrace(data, parts[1]);
  else if (top === "memory") renderMemory(data);
  else if (top === "api-keys") renderApiKeys(data);
  else if (top === "evals") renderEvals(data);
  else if (top === "exports") renderExports(data);
  else if (top === "citations") renderCitation(data, parts[1]);
  else renderHome(data);
  wireCopyButtons();
}

async function main() {
  const response = await fetch("/api/fixture/launch-surface.json", {
    headers: { accept: "application/json" }
  });
  const data = await response.json();
  route(data);
}

window.addEventListener("popstate", main);
main().catch((error) => {
  content.innerHTML = `<section class="panel"><h1>Launch surface failed</h1><pre><code>${escapeHtml(error.message)}</code></pre></section>`;
});
