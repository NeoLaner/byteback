#!/usr/bin/env node
"use strict";

// Launcher: locate the native binary that npm/pnpm installed via the matching
// `optionalDependency` (selected automatically by its `os`/`cpu` fields), then
// exec it — passing argv and the TTY through so the dashboard works as if run
// directly. No install-time scripts, so every package manager works the same.

const { spawnSync } = require("child_process");

// platform+arch -> the optional dependency that ships that binary.
const PLATFORM_PACKAGES = {
  "linux x64": "byteback-linux-x64",
  "linux arm64": "byteback-linux-arm64",
  "darwin x64": "byteback-darwin-x64",
  "darwin arm64": "byteback-darwin-arm64",
  "win32 x64": "byteback-win32-x64",
};

function resolveBinary() {
  const key = `${process.platform} ${process.arch}`;
  const pkg = PLATFORM_PACKAGES[key];
  if (!pkg) {
    throw new Error(
      `no prebuilt binary for ${key}. ` +
        `See https://github.com/NeoLaner/byteback/releases, or install with cargo.`
    );
  }
  const exe = process.platform === "win32" ? "byteback.exe" : "byteback";
  try {
    return require.resolve(`${pkg}/bin/${exe}`);
  } catch {
    throw new Error(
      `the '${pkg}' package is missing. Your installer may have skipped ` +
        `optional dependencies — reinstall byteback.`
    );
  }
}

let binary;
try {
  binary = resolveBinary();
} catch (err) {
  console.error(`byteback: ${err.message}`);
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });
if (result.error) {
  console.error(`byteback: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
