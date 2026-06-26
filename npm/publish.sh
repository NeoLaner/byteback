#!/usr/bin/env bash
# Assemble and publish the byteback npm packages for a release.
#
# For each platform: download the prebuilt binary from the GitHub release for
# this tag, drop it into the matching platform package, and publish it. Then
# publish the main `byteback` package (whose optionalDependencies point at them).
#
# Env: VERSION (the git tag, e.g. v0.1.2) and REPO (owner/name). Requires
# NODE_AUTH_TOKEN for `npm publish`. Run from the `npm/` directory.
set -euo pipefail

tag="${VERSION}"
version="${tag#v}"
repo="${REPO}"

# pkg-suffix : release target triple : binary name inside the tarball
platforms=(
  "linux-x64:x86_64-unknown-linux-musl:byteback"
  "linux-arm64:aarch64-unknown-linux-gnu:byteback"
  "darwin-x64:x86_64-apple-darwin:byteback"
  "darwin-arm64:aarch64-apple-darwin:byteback"
  "win32-x64:x86_64-pc-windows-msvc:byteback.exe"
)

# Publish each platform package first, so the main package's optional deps exist.
for entry in "${platforms[@]}"; do
  IFS=":" read -r suffix triple binary <<<"$entry"
  pkgdir="platforms/$suffix"
  echo "==> packaging byteback-$suffix ($triple)"

  rm -rf "$pkgdir/bin"
  mkdir -p "$pkgdir/bin"
  url="https://github.com/$repo/releases/download/$tag/byteback-$triple.tar.gz"
  curl -fsSL "$url" | tar -xz -C "$pkgdir/bin"
  test -f "$pkgdir/bin/$binary" || { echo "missing $binary after extract"; exit 1; }

  (
    cd "$pkgdir"
    npm version "$version" --no-git-tag-version --allow-same-version
    npm publish --access public
  )
done

# Sync and publish the main package: pin every optionalDependency to this
# release (done in Node so scoped names like @scope/pkg are handled safely).
npm version "$version" --no-git-tag-version --allow-same-version
VER="$version" node -e "
const fs = require('fs');
const pkg = require('./package.json');
for (const dep of Object.keys(pkg.optionalDependencies || {})) {
  pkg.optionalDependencies[dep] = process.env.VER;
}
fs.writeFileSync('package.json', JSON.stringify(pkg, null, 2) + '\n');
"
npm publish --access public

echo "published byteback@$version and ${#platforms[@]} platform packages"
