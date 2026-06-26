#!/usr/bin/env node
"use strict";

// Thin launcher: exec the native binary fetched by install.js, passing through
// argv and the TTY so the full-screen dashboard works as if run directly.

const fs = require("fs");
const path = require("path");
const { spawnSync } = require("child_process");

const binary = path.join(
  __dirname,
  process.platform === "win32" ? "byteback.exe" : "byteback"
);

if (!fs.existsSync(binary)) {
  console.error(
    "byteback: native binary not found. Reinstall with `npm install -g byteback`."
  );
  process.exit(1);
}

const result = spawnSync(binary, process.argv.slice(2), { stdio: "inherit" });

if (result.error) {
  console.error(`byteback: ${result.error.message}`);
  process.exit(1);
}
process.exit(result.status === null ? 1 : result.status);
