const args = process.argv.slice(2);
const defaultDb = "bench.db";
const dbIndex = args.indexOf("--db");
const benchArgs = dbIndex < 0 ? ["--db", defaultDb, ...args] : args;

const proc = Bun.spawn(
  ["cargo", "run", "-p", "tidemail-bench", "--release", "--bin", "bench", "--", ...benchArgs],
  { stdout: "inherit", stderr: "inherit" },
);
process.exit(await proc.exited);
