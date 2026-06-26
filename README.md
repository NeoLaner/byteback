# byteback

> Get your bytes back.

A fast terminal UI that reclaims disk space by finding and sweeping the
regenerable build, dependency, and cache directories that pile up across your
projects — `node_modules`, `.next`, `.turbo`, `target`, `dist`, and friends.

## Features

- **Full-screen dashboard** — pick which directory names to sweep, scan a tree,
  and review every match before anything is deleted.
- **Persistent, customizable targets** — add your own directory names (saved
  forever) and remove built-ins you never want to see again.
- **A clear overview** — found directories are grouped by category with path,
  size, and file count, sorted biggest-first.
- **Safe by default** — everything is selected for you, but you opt out of any
  dir before deleting, and deletions go to the **system trash** unless you
  switch to permanent.
- **Fast** — parallel directory walking (jwalk + rayon) that prunes at every
  match, so it scans your project tree, not the insides of `node_modules`.

## Install

### AUR (Arch Linux)

```bash
yay -S byteback-bin     # or: paru -S byteback-bin
```

### npm / pnpm / yarn

```bash
npm install -g byteback     # or: pnpm add -g byteback / yarn global add byteback
```

The prebuilt binary for your platform is delivered as a per-platform
`optionalDependency` (no install scripts), so it works the same across npm, pnpm,
and yarn. Provides both `byteback` and the short `bb`.

### From source / cargo

```bash
cargo install --git https://github.com/NeoLaner/byteback
# or, from a clone:
git clone https://github.com/NeoLaner/byteback.git && cd byteback && cargo install --path .
```

### Arch, while AUR signups are paused

You can build the AUR package straight from this repo without an AUR account:

```bash
git clone https://github.com/NeoLaner/byteback.git
cd byteback/packaging/aur && makepkg -si
```

## Usage

```bash
byteback            # scan the current directory
byteback ~/code     # scan a specific directory
byteback --permanent # default to permanent deletion instead of trash
```

The dashboard walks you through it:

| Screen | Keys |
| --- | --- |
| **Select targets** | `↑↓` move · `space` toggle · `a` add a custom name · `r` remove a name · `c` change scan directory · `enter` scan |
| **Review results** | `↑↓` move · `space` opt a directory out · `a` all · `n` none · `t`/`p` trash / permanent · `d` delete · `b` back |
| Anywhere | `q` quit |

After deleting you get a summary of the space reclaimed (and anything that
couldn't be removed).

## Configuration

Your custom names, hidden defaults, and last selection are stored at:

- Linux: `~/.config/byteback/config.toml`
- macOS: `~/Library/Application Support/byteback/config.toml`
- Windows: `%APPDATA%\byteback\config.toml`

## How it works

1. **Discover** — walk the chosen directory, recording any folder whose name is a
   target and *not* descending into it (a nested `node_modules` inside another is
   counted once, via the outer dir).
2. **Measure** — sum the size and file count of each match in parallel.
3. **Review & delete** — you confirm the selection and disposal mode; matches go
   to the trash (recoverable) or are removed permanently.

## License

MIT — see [LICENSE](LICENSE).
