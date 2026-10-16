# Cleanator

**Disk cleaner for developers who run AI on Linux.** Models, Python environments, Docker
images, package caches, AI assistants' histories: Cleanator shows where the space went,
classifies what can go (`safe` / `review` / `never`), and writes a shell script you read and
run yourself. **It never deletes anything.**

> Status: **0.1.0, first release** — see [Releases](https://github.com/blackberrylabs/cleanator/releases)
> and [`CHANGELOG.md`](CHANGELOG.md). Everything the tool knows is in [`rules/`](rules/) — 70 declarative rules, each with its
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

## Languages

The interface is in English and French (system language, switchable in the app). The
explanations attached to each recommendation — *why*, *how it regenerates*, *what it
costs* — come from the rules and are **English only**, whatever the interface language.
More languages are welcome as contributions: add `locales/<lang>.json`, have a native
speaker read it, open a pull request.

## Two front ends, one core

`cleanator` (desktop, iced) is for exploring; `cleanator-cli` is for terminals, SSH
sessions on a headless GPU box, and scripts. Both ship in the same package and share the
same rules, probes and classifier.

## Install

Download the package for your distribution from the [latest release](https://github.com/blackberrylabs/cleanator/releases/latest), then:

```sh
sudo apt install ./cleanator_0.1.0-1_amd64.deb        # Debian, Ubuntu
sudo dnf install ./cleanator-0.1.0-1.x86_64.rpm       # Fedora
chmod +x Cleanator-0.1.0-x86_64.AppImage && ./Cleanator-0.1.0-x86_64.AppImage   # any distro
```

Each package ships `cleanator` (desktop) and `cleanator-cli`, the `.desktop` entry and the
icons. The desktop app renders with Vulkan when available and falls back to software
rendering otherwise. From source: `cargo build --release` with a current stable Rust.

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
- `crates/app` — `cleanator`, the desktop app (iced)
- `rules/` — the rules, `docs/rules.md` — their schema
- `locales/` — interface strings (`en`, `fr`), `assets/` — icon and desktop entry
- `packaging/build.sh` — builds the `.deb`, `.rpm` and AppImage into `dist/`

## License

MIT — Alexandre Do, 2026.
