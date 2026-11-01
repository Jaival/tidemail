// E2E bench: launches the release app with WebView2 remote debugging and
// measures cold launch and list scrolling through Playwright over CDP.
// Run with Node (type stripping): `node e2e/bench.ts --db bench.db [--exe path] [--json]`.
// `bun run bench` builds the app and runs this after the Rust bench.

import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { chromium, type Browser, type Page } from "playwright";

const COLD_LAUNCH_MS = 400;
const SCROLL_DROPPED_PCT = 2;
const SCROLL_BLANK_FRAMES = 0;
const LAUNCHES = 3;
const PORT = 9333;

type Line = { name: string; status: "PASS" | "FAIL" | "SKIP"; value: number; budget: number; unit: string; detail?: string };

function arg(name: string): string | undefined {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

const db = resolve(arg("db") ?? "bench.db");
const exe = resolve(arg("exe") ?? "target/release/tidemail.exe");
for (const [what, path] of [["database", db], ["app", exe]]) {
  if (!existsSync(path)) {
    console.error(`${what} not found: ${path}`);
    process.exit(2);
  }
}

function kill(app: ChildProcess) {
  if (app.pid !== undefined) spawnSync("taskkill", ["/pid", String(app.pid), "/T", "/F"], { stdio: "ignore" });
}

async function connect(app: ChildProcess): Promise<Browser> {
  const deadline = Date.now() + 20_000;
  for (;;) {
    if (app.exitCode !== null) throw new Error(`app exited early (code ${app.exitCode}); is Tidemail already running?`);
    try {
      return await chromium.connectOverCDP(`http://127.0.0.1:${PORT}`);
    } catch (e) {
      if (Date.now() > deadline) throw e;
      await new Promise((r) => setTimeout(r, 25));
    }
  }
}

async function appPage(browser: Browser): Promise<Page> {
  const deadline = Date.now() + 10_000;
  for (;;) {
    const page = browser.contexts().flatMap((c) => c.pages()).find((p) => !p.url().startsWith("devtools:"));
    if (page) return page;
    if (Date.now() > deadline) throw new Error("no app page over CDP");
    await new Promise((r) => setTimeout(r, 25));
  }
}

async function launch(): Promise<{ app: ChildProcess; browser: Browser; page: Page; coldMs: number }> {
  const app = spawn(exe, ["--db", db], {
    env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${PORT}` },
    stdio: "ignore",
  });
  try {
    const browser = await connect(app);
    const page = await appPage(browser);
    const coldMs = await page.waitForFunction(() => window.__tidemailColdLaunchMs, undefined, { timeout: 15_000 })
      .then((h) => h.jsonValue() as Promise<number>);
    return { app, browser, page, coldMs };
  } catch (e) {
    kill(app);
    throw e;
  }
}

/** Scrolls the list a few rows per frame; counts long frames and frames with uncovered rows. */
async function scroll(page: Page) {
  await page.waitForSelector(".message-row");
  return page.evaluate(async () => {
    const list = document.querySelector<HTMLElement>(".message-list")!;
    const FRAMES = 600;
    const STEP = 56 * 3;
    const frame = () => new Promise<number>((r) => requestAnimationFrame(r));
    const covered = () => {
      const box = list.getBoundingClientRect();
      const bottom = Math.min(box.bottom, box.top + list.scrollHeight - list.scrollTop);
      const rows = [...list.querySelectorAll(".message-row")]
        .map((el) => el.getBoundingClientRect())
        .filter((r) => r.bottom > box.top && r.top < bottom)
        .sort((a, b) => a.top - b.top);
      let y = box.top;
      for (const r of rows) {
        if (r.top > y + 1) return false;
        y = Math.max(y, r.bottom);
      }
      return y >= bottom - 1;
    };
    const deltas: number[] = [];
    let blank = 0;
    let last = await frame();
    for (let i = 0; i < FRAMES; i++) {
      // What the previous frame painted, then scroll for the next one.
      if (!covered()) blank++;
      list.scrollTop += STEP;
      const now = await frame();
      deltas.push(now - last);
      last = now;
    }
    const sorted = [...deltas].sort((a, b) => a - b);
    const interval = sorted[Math.floor(sorted.length / 2)];
    const dropped = deltas.filter((d) => d > interval * 1.5).length;
    return { frames: FRAMES, dropped, blank, interval, rows: list.querySelectorAll(".message-row").length, scrolled: list.scrollTop };
  });
}

const lines: Line[] = [];
const colds: number[] = [];
for (let i = 0; i < LAUNCHES; i++) {
  const run = await launch();
  colds.push(run.coldMs);
  if (i === LAUNCHES - 1) {
    const s = await scroll(run.page);
    const pct = (s.dropped / s.frames) * 100;
    lines.push({
      name: "scroll dropped frames",
      status: pct <= SCROLL_DROPPED_PCT ? "PASS" : "FAIL",
      value: pct, budget: SCROLL_DROPPED_PCT, unit: "%",
      detail: `${s.dropped}/${s.frames} frames, ${s.interval.toFixed(1)} ms interval, ${Math.round(s.scrolled)} px`,
    });
    lines.push({
      name: "scroll blank frames",
      status: s.blank <= SCROLL_BLANK_FRAMES ? "PASS" : "FAIL",
      value: s.blank, budget: SCROLL_BLANK_FRAMES, unit: "",
    });
  }
  await run.browser.close().catch(() => undefined);
  kill(run.app);
  await new Promise((r) => setTimeout(r, 500));
}
const worst = Math.max(...colds);
lines.unshift({
  name: "cold launch (worst)",
  status: worst < COLD_LAUNCH_MS ? "PASS" : "FAIL",
  value: worst, budget: COLD_LAUNCH_MS, unit: "ms",
  detail: colds.map((c) => `${c} ms`).join(", "),
});
// j/k needs the reader (Phase 1 UI shell + renderer).
lines.push({ name: "j/k open to paint", status: "SKIP", value: 0, budget: 50, unit: "ms" });

if (process.argv.includes("--json")) {
  console.log(JSON.stringify(lines));
} else {
  console.log(`${"e2e bench".padEnd(30)} ${"status".padStart(6)} ${"value".padStart(9)} ${"budget".padStart(9)}`);
  for (const l of lines) {
    const v = `${l.value.toFixed(l.unit === "%" ? 2 : 0)}${l.unit}`;
    console.log(`${l.name.padEnd(30)} ${l.status.padStart(6)} ${v.padStart(9)} ${`${l.budget}${l.unit}`.padStart(9)}  ${l.detail ?? ""}`);
  }
}
process.exit(lines.some((l) => l.status === "FAIL") ? 1 : 0);
