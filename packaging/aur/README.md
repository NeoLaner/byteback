# AUR packaging for `byteback`

`byteback-bin` installs the prebuilt, statically linked (musl) binary from the
GitHub release, plus a `bb` symlink.

## Publishing a new version

The AUR package lives in its own git repo (`ssh://aur@aur.archlinux.org/byteback-bin.git`),
separate from this source repo. After a `vX.Y.Z` release exists on GitHub:

1. Bump `pkgver` in `PKGBUILD` to match the release.
2. Fill in real checksums (replaces the `SKIP` placeholders):
   ```bash
   updpkgsums
   ```
3. Regenerate the metadata and sanity-check the build:
   ```bash
   makepkg --printsrcinfo > .SRCINFO
   makepkg -si   # builds + installs locally to verify
   ```
4. Push to the AUR:
   ```bash
   git clone ssh://aur@aur.archlinux.org/byteback-bin.git aur-byteback
   cp PKGBUILD .SRCINFO aur-byteback/
   cd aur-byteback && git commit -am "byteback-bin X.Y.Z" && git push
   ```

> The `aur-byteback` clone is outside this project; create it wherever you keep
> your AUR checkouts.
