# Cleanator rules

Every recommendation Cleanator makes comes from a rule in `rules/*.toml`. Rules are
declarative, published with the code, and open to challenge: if you think a rule is wrong,
open an issue quoting its `id` and the documentation that contradicts it.

Three principles (from the project brief):

1. **Cleanator never deletes anything.** A rule describes *how* something could be
   removed; the generated script is what the user reads and runs.
2. **What Cleanator does not know is `never`.** A path or file matched by no rule is never
   proposed.
3. **Managed stores are cleaned by their own tool.** If a tool keeps an index (Ollama,
   Docker, uv, pnpm, Hugging Face, snap, flatpak, apt, journald), the rule calls that
   tool's command, never `rm` inside the store.

## File layout

One file per category, `schema = 1` at the top, then an array of `[[rule]]` tables.

```toml
schema = 1

[[rule]]
id = "pip-cache"            # unique across all files, kebab-case
name = "pip cache"          # default label; locales/*.json may override by id
category = "package-cache"  # see Categories
level = "safe"              # safe | review | never

[rule.detect]
kind = "path"               # path | signature | pattern | probe
paths = ["$PIP_CACHE_DIR", "$XDG_CACHE_HOME/pip", "~/.cache/pip"]

[rule.cleanup]
kind = "command"            # command | rm | trash | none
command = "pip cache purge"
fallback = "rm"             # used when the tool is not installed (optional)
requires = "user"           # user | sudo

[rule.explain]
why = "HTTP responses and locally built wheels kept to speed up later installs."
regenerate = "pip downloads or rebuilds what it needs on the next install."
cost = "seconds to minutes, network"

[rule.source]
url = "https://pip.pypa.io/en/stable/topics/caching/"
read_on = 2026-10-15
quote = "Pip will also respect XDG_CACHE_HOME."
```

## Levels

| level | meaning | script action |
|---|---|---|
| `safe` | regenerable without loss: caches, build outputs, indexes | `rm -rf` or the tool's command; space is freed immediately |
| `review` | recoverable but costly or ambiguous: models, environments, backups, unused images | moved to the trash (`gio trash`) or the tool's command; the user must tick it explicitly |
| `never` | configuration, credentials, user data, anything in use, anything unknown | shown in the treemap only, never in the script |

A rule may raise its level under a condition (never lower it):

```toml
[rule.conditions]
active_project_level = "review"   # pattern rules: project modified within dormancy window
dormancy_days = 60                # default 60, user-adjustable in the UI
dormancy_measure = "atime:bin/python"  # mtime (default) | atime:<relative file>
```

Items younger than the dormancy window are not proposed by `review` rules; `safe` rules
ignore dormancy unless they set one (caches of a session that just ended, for instance).
`dormancy_measure = "atime"` uses the last access (for a model: the last time it was
loaded), `"mtime"` the last modification, `"atime:bin/python"` a specific file's access.

Access times are evidence only when the filesystem records them. Two checks protect
atime-measured `review` rules (since 0.1.1):

- **`noatime` mount** (read from `/proc/mounts`): reads are never recorded, so the last use
  is *unknown*. The item is classified and shown but never proposed, with a note naming the
  mount; the report header says how many items and bytes are concerned. Unknown is not
  proposed (project rule R2).
- **Mass-read day**: when one day other than today or yesterday holds at least 40 % of the
  large files' access times (and at least 200 of them), something read most of the disk at
  once — an indexer, a backup, an antivirus. An item last read that day has a real last use
  somewhere between its modification and that day, so it is dormant only once *that day* is
  older than the window. The note says so and gives the date it becomes proposable.

The Linux default `relatime` is reliable for a window of weeks: the access time is updated
as soon as it is more than a day old.

## Sources

Every rule cites where its knowledge comes from. `source.url` is the documentation page
that was read, `read_on` the date, `quote` the sentence that supports the rule (the UI
shows the link as "docs"). Two special values:

- `to-verify` — the rule was written from experience and its documentation has not been
  read yet. Allowed while developing, counted by `cleanator-cli rules`; a release ships
  with none.
- `observed` — no public documentation describes this layout. `quote` then states what
  was seen, on what system and when, so a reader can tell observation from documentation
  and a contributor can replace it with a reference when one appears.

## One owner per path

Rules run in file order (files sorted by name), the catch-all rule (`extensions = ["*"]`)
last. **The first rule to classify a path keeps it**: a later rule skips that path and
anything below it. A model directory claimed by the Hugging Face probe is not re-listed by
the loose-model rule; a repository claimed as re-clonable is not re-listed piece by piece;
the catch-all only lists what nothing else classified.

## Detection kinds

- **`path`** — a fixed location. `paths` is tried in order; `$VAR` entries whose variable is
  unset are skipped; the first existing path wins. `~` is the user's home.
- **`signature`** — files recognised by what they are, wherever they are:
  `extensions`, optional `min_size`. A signature file inside a managed store is attributed
  to that store's rule, not to the signature rule (`exclude_managed = true`).
- **`pattern`** — a directory name found anywhere (`glob = "**/node_modules"`, or a list of
  globs), or a directory recognised by a file it contains (`marker = "pyvenv.cfg"`),
  attributed to its parent project (`anchor = "project"`: nearest ancestor with `.git`,
  `package.json`, `pyproject.toml`, `Cargo.toml`). `requires_sibling` restricts a match to
  directories whose parent holds one of the listed files (`dist/` only next to a
  `package.json`). `exclude_under` skips matches below the listed path fragments;
  `skip_in_project = true` skips matches that sit inside a project. Project
  activity = newest mtime outside build dirs. `group_by` chooses how items are grouped in
  the UI (`project`, `app`, `parent:N`).
- **`probe`** — the item is listed by a tool, not by walking the disk (`probe = "docker"`,
  `item = "build-cache"`). Probes are implemented in the Rust core; a rule only names them.

## In-use checks

Cleanator never proposes something in use. Checks are declared per rule and evaluated by
the core:

```toml
[rule.in_use]
processes = ["ollama"]        # if any of these processes runs, items may be loaded
probe = "ollama-ps"           # a probe that lists loaded items
open_files = true             # any file held open by a process → not proposed
bind_mounts = true            # any path mounted into a running container → not proposed
```

`open_files` and `bind_mounts` default to `true` for every `review` rule.

## Sizes

Sizes shown are **allocated blocks**, hard links counted once per inode. For Docker the
size is the image's *unique* size (what `docker system df -v` calls `UNIQUE SIZE`), never
the virtual size. Caches that hard-link into environments (uv, pnpm) show "shared with N
environments" and warn that cleaning frees less than displayed.

## Categories

`package-cache`, `build`, `python-env`, `model`, `dataset`, `training-output`, `docker`,
`assistant-history`, `assistant-cache`, `assistant-config`, `tooling`, `browser`,
`system-index`, `backup`, `git-reclonable`, `large-file`, `system`, `snapshot`.

## What the UI shows, per rule

A recommendation row shows `name`, size, level badge and the one-line `why`. Expanded, it
shows the exact command or path the script will use, `regenerate`, `cost`, last use, the
`source.url` as a "docs" link and the rule `id` (to quote in an issue). The generated
script repeats `why` as a comment above each command. Same data, two depths: the first is
enough for most users, the second is what a developer checks before ticking.

## Privacy

Rules only ever describe paths, names, sizes and dates. Cleanator never reads file
contents (project brief, R10 bis). Credential and configuration files are matched by
`never` rules so they are excluded explicitly rather than by accident.
