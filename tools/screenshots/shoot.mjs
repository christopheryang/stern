// Copyright (c) 2026 Christopher Yang
// SPDX-License-Identifier: MIT
//
// Take one screenshot of a page in headless Chrome, once the page says
// it is ready (document.title === "ready", set by capture-stub.js).
//
//   node shoot.mjs <chrome> <url> <out.png> [width] [height]
//
// Talks to Chrome over its DevTools protocol using Node's built-in
// WebSocket, so nothing needs installing.

import { spawn } from "node:child_process";
import { writeFileSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const [chrome, url, out, w = "900", h = "700"] = process.argv.slice(2);
const width = Number(w);
const height = Number(h);
const profile = mkdtempSync(join(tmpdir(), "stern-shot-"));
const proc = spawn(
  chrome,
  [
    "--headless=new", "--disable-gpu", "--hide-scrollbars", "--no-first-run",
    `--user-data-dir=${profile}`, "--remote-debugging-port=0",
    `--window-size=${width},${height}`, "about:blank",
  ],
  { stdio: ["ignore", "ignore", "pipe"] },
);

const cleanup = () => {
  proc.kill("SIGKILL");
  // Chrome may still be writing into its profile as it dies; a leftover
  // temp directory is harmless, so never let this fail the capture.
  try {
    rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 100 });
  } catch { /* the OS clears its temp directory */ }
};
process.on("exit", cleanup);

const wsUrl = await new Promise((resolve, reject) => {
  const timer = setTimeout(() => reject(new Error("Chrome did not start")), 15000);
  let buf = "";
  proc.stderr.on("data", (d) => {
    buf += d;
    const m = buf.match(/DevTools listening on (ws:\/\/\S+)/);
    if (m) { clearTimeout(timer); resolve(m[1]); }
  });
});

const sock = new WebSocket(wsUrl);
await new Promise((r) => (sock.onopen = r));
let nextId = 1;
const pending = new Map();
sock.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.id && pending.has(msg.id)) {
    const { resolve, reject } = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error ? reject(new Error(msg.error.message)) : resolve(msg.result);
  }
};
const send = (method, params = {}, sessionId) =>
  new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    sock.send(JSON.stringify({ id, method, params, sessionId }));
  });

const { targetInfos } = await send("Target.getTargets");
const target = targetInfos.find((t) => t.type === "page");
const { sessionId } = await send("Target.attachToTarget", { targetId: target.targetId, flatten: true });
const call = (m, p) => send(m, p, sessionId);

await call("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: 1, mobile: false });
await call("Page.enable");
await call("Page.navigate", { url });

const deadline = Date.now() + 40000;
let ready = false;
while (Date.now() < deadline) {
  const r = await call("Runtime.evaluate", { expression: "document.title", returnByValue: true });
  if (r.result.value === "ready") { ready = true; break; }
  await new Promise((r2) => setTimeout(r2, 150));
}
if (!ready) throw new Error(`page never became ready: ${url}`);
await new Promise((r) => setTimeout(r, 250));

const shot = await call("Page.captureScreenshot", { format: "png" });
writeFileSync(out, Buffer.from(shot.data, "base64"));
// Chrome's helper processes keep the event loop alive, so exit
// explicitly; the exit handler kills Chrome and removes its profile.
process.exit(0);
