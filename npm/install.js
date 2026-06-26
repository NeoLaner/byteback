#!/usr/bin/env node
"use strict";

// Postinstall step: download the prebuilt `byteback` binary that matches this
// platform from the matching GitHub release and unpack it into ./bin.
//
// This mirrors what tools like esbuild/biome do: the npm package is a thin
// launcher, and the native binary is fetched on install rather than vendored.

const fs = require("fs");
const os = require("os");
const path = require("path");
const https = require("https");
const { spawnSync } = require("child_process");

const REPO = "neolaner/byteback";
const { version } = require("./package.json");

// node platform+arch -> Rust target triple shipped in the release.
const TARGETS = {
  "linux x64": "x86_64-unknown-linux-musl",
  "linux arm64": "aarch64-unknown-linux-gnu",
  "darwin x64": "x86_64-apple-darwin",
  "darwin arm64": "aarch64-apple-darwin",
  "win32 x64": "x86_64-pc-windows-msvc",
};

function targetTriple() {
  const key = `${process.platform} ${process.arch}`;
  const triple = TARGETS[key];
  if (!triple) {
    throw new Error(
      `byteback has no prebuilt binary for ${key}. ` +
        `See https://github.com/${REPO}/releases or build from source with cargo.`
    );
  }
  return triple;
}

function binaryName() {
  return process.platform === "win32" ? "byteback.exe" : "byteback";
}

function download(url, dest) {
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);
    const request = https.get(url, { headers: { "User-Agent": "byteback-installer" } }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        file.close();
        fs.rmSync(dest, { force: true });
        resolve(download(res.headers.location, dest));
        return;
      }
      if (res.statusCode !== 200) {
        file.close();
        fs.rmSync(dest, { force: true });
        reject(new Error(`download failed (${res.statusCode}) for ${url}`));
        return;
      }
      res.pipe(file);
      file.on("finish", () => file.close(() => resolve()));
    });
    request.on("error", (err) => {
      file.close();
      fs.rmSync(dest, { force: true });
      reject(err);
    });
  });
}

function extract(archive, dir) {
  // `tar` ships with Linux, macOS, and Windows 10+ (bsdtar) and auto-detects gzip.
  const result = spawnSync("tar", ["-xzf", archive, "-C", dir], { stdio: "inherit" });
  if (result.status !== 0) {
    throw new Error(`failed to extract ${archive}`);
  }
}

async function main() {
  const triple = targetTriple();
  const binDir = path.join(__dirname, "bin");
  fs.mkdirSync(binDir, { recursive: true });

  const asset = `byteback-${triple}.tar.gz`;
  const url = `https://github.com/${REPO}/releases/download/v${version}/${asset}`;
  const archive = path.join(os.tmpdir(), `byteback-${version}-${triple}.tar.gz`);

  console.log(`byteback: downloading ${asset} ...`);
  await download(url, archive);
  extract(archive, binDir);
  fs.rmSync(archive, { force: true });

  const binary = path.join(binDir, binaryName());
  if (!fs.existsSync(binary)) {
    throw new Error(`binary ${binaryName()} missing after extraction`);
  }
  if (process.platform !== "win32") {
    fs.chmodSync(binary, 0o755);
  }
  console.log(`byteback ${version} installed.`);
}

// Exported for tests; only runs the download when invoked directly.
module.exports = { targetTriple, binaryName, TARGETS };

if (require.main === module) {
  main().catch((err) => {
    console.error(`byteback install failed: ${err.message}`);
    process.exit(1);
  });
}
