import http from "node:http";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { readFile } from "node:fs/promises";

const port = Number(process.env.PORT || 4173);
const root = path.join(path.dirname(fileURLToPath(import.meta.url)), "public");

const contentTypes = {
  ".css": "text/css; charset=utf-8",
  ".html": "text/html; charset=utf-8",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".svg": "image/svg+xml"
};

function resolvePublicPath(requestPath) {
  const decoded = decodeURIComponent(requestPath);
  const normalized = path.normalize(decoded).replace(/^(\.\.[/\\])+/, "");
  const direct = path.join(root, normalized);
  if (!direct.startsWith(root)) {
    return null;
  }
  if (path.extname(direct)) {
    return direct;
  }
  return path.join(root, "index.html");
}

async function handle(request, response) {
  const url = new URL(request.url || "/", `http://${request.headers.host || "localhost"}`);
  const filePath = resolvePublicPath(url.pathname);
  if (!filePath) {
    response.writeHead(403).end("forbidden");
    return;
  }
  try {
    const body = await readFile(filePath);
    response.writeHead(200, {
      "content-type": contentTypes[path.extname(filePath)] || "application/octet-stream",
      "cache-control": "no-store"
    });
    response.end(body);
  } catch {
    if (!path.extname(filePath)) {
      response.writeHead(404).end("not found");
      return;
    }
    const body = await readFile(path.join(root, "index.html"));
    response.writeHead(200, { "content-type": contentTypes[".html"], "cache-control": "no-store" });
    response.end(body);
  }
}

http.createServer(handle).listen(port, "127.0.0.1", () => {
  process.stdout.write(`memphant-web=http://127.0.0.1:${port}\n`);
});
