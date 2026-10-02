import { createServer } from "node:http";

const port = Number(process.env.PORT || 3000);
const lane = process.env.LANE || "without-lanes";
const baseUrl = process.env.BASE_URL || `http://localhost:${port}`;

if (!Number.isInteger(port) || port < 1 || port > 65535) {
  console.error("PORT must be an integer from 1 to 65535");
  process.exit(1);
}

const escapeHtml = (value) => String(value).replace(/[&<>"']/g, (char) => ({
  "&": "&amp;",
  "<": "&lt;",
  ">": "&gt;",
  '"': "&quot;",
  "'": "&#39;",
})[char]);

createServer((request, response) => {
  if (request.url === "/health") {
    response.writeHead(200, { "content-type": "application/json; charset=utf-8" });
    response.end(JSON.stringify({ lane, port, baseUrl }));
    return;
  }

  response.writeHead(200, { "content-type": "text/html; charset=utf-8" });
  response.end(`<!doctype html>
<html lang="en">
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(lane)} · Lanes demo</title>
<style>
  * { box-sizing: border-box; }
  body { margin: 0; min-height: 100vh; display: grid; place-items: center; background: #0d1117; color: #f0f6fc; font: 16px system-ui, sans-serif; }
  main { width: min(520px, calc(100vw - 32px)); padding: 32px; background: #161b22; border: 1px solid #30363d; border-radius: 12px; }
  .eyebrow { color: #8b949e; font-size: 12px; font-weight: 700; letter-spacing: .12em; }
  h1 { margin: 14px 0 8px; font-size: 36px; }
  p { color: #8b949e; }
  code { color: #58a6ff; }
  .port { display: inline-block; margin-top: 14px; padding: 8px 12px; border-radius: 6px; background: #0d1117; color: #3fb950; font: 600 18px ui-monospace, monospace; }
</style>
<main>
  <div class="eyebrow">LANES · BRANCH DEMO</div>
  <h1>${escapeHtml(lane)}</h1>
  <p>This worktree has its own stable local address.</p>
  <code>${escapeHtml(baseUrl)}</code><br>
  <span class="port">PORT=${port}</span>
</main>
</html>`);
}).listen(port, "127.0.0.1", () => {
  console.log(`${lane} → ${baseUrl}`);
});
