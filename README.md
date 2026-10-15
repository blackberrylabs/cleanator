# Cleanator

**Disk cleaner for developers who run AI on Linux.** Models, Python environments, Docker
images, package caches, AI assistants' histories: Cleanator shows where the space went,
classifies what can go (`safe` / `review` / `never`), and writes a shell script you read and
run yourself. **It never deletes anything.**

> Status: **pre-release.** The core (`cleanator-cli`) works; the desktop app is being built.
> Everything the tool knows is in [`rules/`](rules/) — 70 declarative rules, each with its
> reason, how the item regenerates, what it costs, and the documentation it is based on.
> Disagree with one? Open an issue quoting its `id`.

## Why another cleaner

Baobab shows sizes but does not know what a `node_modules` is. BleachBit cleans but has
never heard of Ollama, Hugging Face, uv, Cursor or Claude Code. On a developer's machine
the hundreds of GB are in models, virtual environments, Docker layers and assistant caches,
and the dangerous mistakes are there too (deleting an Ollama blob by hand, trashing a venv
that is bind-mounted into a running container). Cleanator was written for that disk.

## Principles

1. **Never deletes.** It writes a script: one block per item, the reason as a comment,
   the exact command, `set -euo pipefail`, every path quoted. `safe` items are removed by
   the script, `review` items go to the trash.
2. **What it does not know is `never`.** No rule, no proposal.
3. **Managed stores are cleaned by their own tool.** `ollama rm`, `docker image prune`,
   `uv cache clean`, `hf cache rm`, `snap remove --revision` — never `rm` inside a store
   that keeps an index.
4. **True sizes.** Allocated blocks, hard links counted once, Docker's *unique* size, a
   warning when a cache is shared with environments.
5. **Nothing in use is proposed.** Running processes, open files, paths mounted into
   running containers, models loaded in Ollama.
6. **No network, metadata only.** No telemetry, no update check; file contents are never
   read; credentials and configuration files are matched by explicit `never` rules.

## Try the core

```sh
cargo build --release
./target/release/cleanator-cli rules                 # the 70 rules, validated
./target/release/cleanator-cli scan ~ --depth 1      # sizes, like du -x
./target/release/cleanator-cli recommend ~           # what could go, and why
./target/release/cleanator-cli script ~ -o clean.sh  # read it, then: bash clean.sh
```

`recommend --explain` shows the command behind each item, the `never` items and the rules
that found nothing. `--no-probes` skips the tool probes (docker, ollama, snap…),
`--dormancy-days N` changes the 60-day window.

## Layout

- `crates/core` — scanner, rules engine, probes, classifier, script generator (library)
- `crates/cli` — `cleanator-cli`
- `crates/app` — desktop app (iced), not built yet
- `rules/` — the rules, `docs/rules.md` — their schema

## License

MIT — Alexandre Do, 2026.
