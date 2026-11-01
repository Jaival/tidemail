// `bun run bench [-- --db bench.db] [--skip-e2e]`: Rust bench, then the E2E bench.
// Fails if any budget is missed.
const args = process.argv.slice(2);
const skipE2e = args.includes("--skip-e2e");
const rest = args.filter((a) => a !== "--skip-e2e");
const benchArgs = rest.includes("--db") ? rest : ["--db", "bench.db", ...rest];

async function run(cmd: string[]): Promise<number> {
  const proc = Bun.spawn(cmd, { stdout: "inherit", stderr: "inherit" });
  return proc.exited;
}

let failed = (await run(["cargo", "run", "-p", "tidemail-bench", "--release", "--bin", "bench", "--", ...benchArgs])) !== 0;

if (!skipE2e) {
  // Production build (embedded frontend); `cargo build` alone would load the dev server.
  if ((await run(["bunx", "tauri", "build", "--no-bundle"])) !== 0) process.exit(1);
  // Playwright over CDP runs under Node, not Bun's runtime.
  failed = (await run(["node", "e2e/bench.ts", ...benchArgs])) !== 0 || failed;
}
process.exit(failed ? 1 : 0);
