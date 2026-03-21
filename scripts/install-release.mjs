#!/usr/bin/env node

import { chmod, copyFile, mkdir, rename, rm } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { spawn } from "node:child_process";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "..");
const INSTALL_DIR = process.env.RUPASS_INSTALL_DIR || path.join(os.homedir(), ".local", "bin");

async function main() {
  const arg = process.argv[2];

  if (arg === "--help" || arg === "-h") {
    printHelp();
    return;
  }

  if (arg) {
    throw new Error(`unexpected argument: ${arg}`);
  }

  const binaryName = process.platform === "win32" ? "rupass.exe" : "rupass";
  const source = path.join(REPO_ROOT, "target", "release", binaryName);
  const destination = path.join(INSTALL_DIR, binaryName);
  const tempPath = `${destination}.tmp`;

  await run("cargo", ["build", "--release", "--locked"], REPO_ROOT);
  await mkdir(INSTALL_DIR, { recursive: true });
  await copyFile(source, tempPath);

  if (process.platform !== "win32") {
    await chmod(tempPath, 0o755);
  }

  await rename(tempPath, destination).catch(async () => {
    await rm(destination, { force: true });
    await rename(tempPath, destination);
  });

  console.log(`installed local release -> ${destination}`);

  if (!isInPath(INSTALL_DIR)) {
    console.log(`note: ${INSTALL_DIR} is not in PATH`);
  }
}

async function run(command, args, cwd) {
  await new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd,
      stdio: "inherit",
      shell: process.platform === "win32",
    });

    child.on("error", reject);
    child.on("exit", (code) => {
      if (code === 0) {
        resolve();
        return;
      }

      reject(new Error(`command failed: ${command} ${args.join(" ")}`));
    });
  });
}

function isInPath(dir) {
  const pathValue = process.env.PATH || "";
  const parts = pathValue.split(path.delimiter);
  return parts.includes(dir);
}

function printHelp() {
  console.log(`Usage:
  pnpm install:release

This command will:
  1. run cargo build --release --locked
  2. install the built binary for your current OS

Environment variables:
  RUPASS_INSTALL_DIR  Install directory, default: ${INSTALL_DIR}
`);
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
