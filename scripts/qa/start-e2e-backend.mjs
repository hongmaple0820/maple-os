import { mkdirSync, rmSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..", "..");
const tmpDir = resolve(repoRoot, ".tmp");
const dbBase = resolve(tmpDir, "mapleos-e2e");
// Allow overriding the cargo target dir via env (used in CI to share the
// build cache across runs). Falls back to a per-repo isolated dir.
const cargoTargetDir = process.env.CARGO_TARGET_DIR
  ? resolve(process.env.CARGO_TARGET_DIR)
  : resolve(tmpDir, "cargo-target-e2e");

mkdirSync(tmpDir, { recursive: true });
mkdirSync(cargoTargetDir, { recursive: true });
for (const suffix of [".db", ".db-shm", ".db-wal"]) {
  rmSync(`${dbBase}${suffix}`, { force: true });
}

const child = spawn(
  "cargo",
  ["run", "-p", "mapleos-server"],
  {
    cwd: repoRoot,
    stdio: "inherit",
    windowsHide: true,
    env: {
      ...process.env,
      HOST: "127.0.0.1",
      PORT: "7788",
      DATABASE_URL: `sqlite:${dbBase}.db?mode=rwc`,
      CARGO_TARGET_DIR: cargoTargetDir,
      REQUIRE_AUTH: "true",
      RUST_LOG: process.env.RUST_LOG ?? "info,mapleos_server=debug",
    },
  },
);

const forwardSignal = (signal) => {
  if (!child.killed) {
    child.kill(signal);
  }
};

process.on("SIGINT", () => forwardSignal("SIGINT"));
process.on("SIGTERM", () => forwardSignal("SIGTERM"));

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
