import { mkdirSync, rmSync, existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..", "..");
const tmpDir = resolve(repoRoot, ".tmp");
const dbBase = resolve(tmpDir, "mapleos-e2e");
const cargoTargetDir = process.env.CARGO_TARGET_DIR
  ? resolve(process.env.CARGO_TARGET_DIR)
  : resolve(tmpDir, "cargo-target-e2e");

mkdirSync(tmpDir, { recursive: true });
for (const suffix of [".db", ".db-shm", ".db-wal"]) {
  rmSync(`${dbBase}${suffix}`, { force: true });
}

const defaultTargetDir = resolve(repoRoot, "target");
const debugBin = resolve(defaultTargetDir, "debug", "mapleos-server");
const releaseBin = resolve(defaultTargetDir, "release", "mapleos-server");

let cmd, args, env;
if (process.env.MAPLEOS_SERVER_BIN) {
  cmd = process.env.MAPLEOS_SERVER_BIN; args = []; env = {};
  console.log(`[start-e2e-backend] using MAPLEOS_SERVER_BIN=${cmd}`);
} else if (existsSync(releaseBin)) {
  cmd = releaseBin; args = []; env = {};
  console.log(`[start-e2e-backend] using pre-built release binary: ${cmd}`);
} else if (existsSync(debugBin)) {
  cmd = debugBin; args = []; env = {};
  console.log(`[start-e2e-backend] using pre-built debug binary: ${cmd}`);
} else {
  mkdirSync(cargoTargetDir, { recursive: true });
  cmd = "cargo"; args = ["run", "-p", "mapleos-server"]; env = { CARGO_TARGET_DIR: cargoTargetDir };
  console.log(`[start-e2e-backend] no pre-built binary found; using cargo run`);
}

const child = spawn(cmd, args, {
  cwd: repoRoot, stdio: "inherit", windowsHide: true,
  env: {
    ...process.env, ...env,
    HOST: "127.0.0.1", PORT: "7788",
    DATABASE_URL: `sqlite:${dbBase}.db?mode=rwc`,
    REQUIRE_AUTH: "false", MAPLEOS_MOCK_LLM: "true",
    RUST_LOG: process.env.RUST_LOG ?? "info,mapleos_server=debug",
  },
});

const forwardSignal = (signal) => { if (!child.killed) child.kill(signal); };
process.on("SIGINT", () => forwardSignal("SIGINT"));
process.on("SIGTERM", () => forwardSignal("SIGTERM"));
child.on("exit", (code, signal) => {
  if (signal) { process.kill(process.pid, signal); return; }
  process.exit(code ?? 1);
});
