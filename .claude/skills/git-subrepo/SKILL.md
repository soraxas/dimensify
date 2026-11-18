---
name: git-subrepo
description: How to inspect, pull, push, edit and upstream the vendored crates under vendor/ that are tracked with git-subrepo-rs (`git subrepo`). Use whenever a task touches vendor/*/.gitrepo, asks where a vendored crate tracks upstream, wants to sync/pull/push a vendored crate, contribute a vendor patch upstream as a PR, or repair "parent not in history" warnings.
---

# git-subrepo-rs in Dimensify

`vendor/<crate>/` directories are **git-subrepos**, not submodules. Their
content is ordinary committed files in this repo; a `vendor/<crate>/.gitrepo`
file records where it came from. The tool is
[soraxas/git-subrepo-rs](https://github.com/soraxas/git-subrepo-rs) (a Rust
reimplementation of ingydotnet/git-subrepo), installed via mise as
`git-subrepo` and callable as `git subrepo <cmd>`. `git subrepo help <cmd>`
prints exact flags; the README on GitHub is the reference for semantics.

## Current subrepos

Read them live rather than trusting this list:

```sh
for f in vendor/*/.gitrepo; do echo "== $f"; git config --file "$f" --get-regexp '^subrepo\.'; done
```

At the time of writing: `bevy_pointcloud`, `bevy_gaussian_splatting`
(branch `LOCAL`), `potree-rs` (`feat/multi-file-octree`), `bevy_editor_cam`
(`0.19`) — all remotes are `github.com/soraxas/<crate>` forks. The original
project is in each crate's `Cargo.toml` `repository` field, and in `.gitrepo`
`upstream` only if someone set it. `vendor/*/LOCAL_PATCHES.md` ledgers record
which local patches are `upstream`, `replayed` or `obsolete`.

## `.gitrepo` fields

| Key | Meaning |
| --- | --- |
| `remote` | Fork URL that `pull`/`push` talk to |
| `branch` | Tracked branch on `remote` |
| `commit` | Remote commit the vendored content was last synced with |
| `parent` | Dimensify HEAD at the last pull/push; used to find local-only commits |
| `method` | `merge` (default) or `rebase` |
| `upstream` | Optional original project when `remote` is a fork (`config` only) |
| `exclude` | Optional, repeatable literal paths kept out of pushes/PR exports |

Never hand-edit `.gitrepo`; use `git subrepo config <subdir> <key> [value]`.

## Read-only inspection (safe, no prompts)

```sh
git subrepo status --no-fetch --no-dirty --all     # pins only, no network
git subrepo status vendor/bevy_pointcloud          # fetches; shows unpushed + new upstream commits
git ls-remote --heads <remote-url>                 # branches on the fork
```

Find Dimensify commits that changed a subrepo since its last sync — these are
local patches not yet pushed to `remote`:

```sh
last=$(git log -1 --format=%H -- vendor/<crate>/.gitrepo)
git log --oneline "$last"..HEAD -- vendor/<crate>
```

`Pull Parent: <sha> (?) ⚠ not in history` means this repo was rebased or
squash-merged since the last sync, so `parent` no longer resolves. `push`
cannot then find local commits reliably. The fix (a `.gitrepo` write) is
`git subrepo config <subdir> parent <sha>` with the Dimensify commit that
last touched `.gitrepo`, or the interactive `git subrepo fix`. Ask before
changing it.

## Writing operations

All of these change this repo or a remote. Confirm with the user first; for
`push` and `pr publish`, always — they publish to GitHub.

- **Edit a vendored crate in place**: just edit files under `vendor/<crate>/`
  and commit normally in Dimensify. Add/update the `LOCAL_PATCHES.md` entry.
  Nothing reaches the fork until a push.
- **Pull fork changes**: `git subrepo pull vendor/<crate>` (add `-b <branch>`
  to switch tracked branch). Squashes into one Dimensify commit and re-pins
  `commit`/`parent`. On conflict it leaves a worktree under
  `.git/tmp/subrepos/...`; resolve there, `git merge --continue`, then
  `git subrepo commit vendor/<crate>`.
- **Push local patches to the fork**: `git subrepo push vendor/<crate>`
  (`-b <branch>` to push elsewhere). The subdir must be clean; it must already
  contain the remote HEAD (pull first). Runs this repo's `pre-push` hook;
  `--no-verify` skips it. Afterwards it commits the updated `.gitrepo`.
- **Work on the crate as a standalone repo**:
  `git subrepo workon vendor/<crate> --no-shell` prints a worktree path
  (`.git/tmp/subrepo/<subdir>`, branch `subrepo/<subdir>`) with the fork wired
  as remote `<subdir>-remote` (and `<subdir>-upstream` if configured). Commit
  there, then `git subrepo commit` (inside it) or
  `git subrepo commit vendor/<crate>` (from root) to import as one Dimensify
  commit. Both checkouts must be clean. Use `-F` to warn if the fork was
  force-pushed past the pin. Don't run `push`/`pull`/`branch -f` on the same
  subdir while unimported workon commits exist — they can be lost.
- **Upstream PR from a focused patch**:
  `git subrepo pr prepare vendor/<crate> --branch <topic> --message '<msg>' [--path <p>]...`,
  review/test the printed export repo, then
  `git subrepo pr publish vendor/<crate> --expected-head <full-sha>`, open the
  PR with `gh`, and after merge
  `git subrepo pr reconcile vendor/<crate> --merged <sha>` (stages an import
  for a separate commit). `--remote`/`--upstream`/`--base` choose fork, origin
  project and target branch. Needs Git ≥ 2.38.
- **Cleanup**: `git subrepo clean vendor/<crate>` removes temporary
  branches/refs/worktrees.

## Gotchas for agents

- Bare `git subrepo` prints help; bare `git subrepo pr` and `fix`/`sync` are
  interactive. Without a TTY, use explicit subcommands and flags.
- Write commands run from a nested directory prompt to confirm the checkout;
  `-y/--yes` approves only that location prompt, not publication or conflicts.
- `-n/--no-edit` avoids an editor for generated commit messages. Hooks are
  skipped on subrepo commits unless `-V/--verify`.
- The `.gitrepo` file is never pushed to the fork; `LOCAL_PATCHES.md` is,
  unless listed in `exclude`.
