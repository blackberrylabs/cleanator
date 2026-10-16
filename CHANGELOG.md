# Changelog

All notable changes to Cleanator. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/);
versions follow [Semantic Versioning](https://semver.org/).

## [0.1.0] — 2026-10-16

First release. A disk cleaner for developers who run AI on Linux: it shows where the space
went, says what can go and why, and writes a script you read and run yourself. **It never
deletes anything.**

### Added

- **70 declarative rules** (`rules/*.toml`), each with its level (`safe` / `review` /
  `never`), the reason, how the item regenerates, what regenerating costs, and the
  documentation it was read from. Covers package caches (pip, uv, npm, npx, pnpm, cargo, conda,
  Playwright, Hugging Face…), Docker (build cache, dangling and unused images, stopped containers,
  volumes), models (Ollama, Hugging Face hub and datasets, LM Studio, loose GGUF/safetensors
  files, local LLM apps), Python
  environments (venv, conda, uv), build outputs (`node_modules`, `target`, `.next`, `dist`,
  `__pycache__`…), AI assistants (Claude Code, Claude Desktop, Codex, Gemini CLI, Cursor,
  Windsurf, Cline/Roo/Kilo, OpenCode, Continue, Aider…), system caches (apt, snap
  revisions, flatpak, journald, tracker, thumbnails, browser caches, crash dumps, GPU
  compile caches, inactive nvm Node versions, CUDA toolkits), backups and large files,
  re-clonable git repositories.
- **Scanner** that measures like `du -x`: allocated blocks, hard links counted once, no
  symlink following, one filesystem, parallel walk (a 636 GB home in about two seconds
  warm).
- **Probes** that ask the tools themselves, read-only: `docker system df`, `ollama ls/ps`,
  `snap list --all`, `journalctl --disk-usage`, `nvm ls`, `conda`, `pnpm`, `apt autoremove`
  dry-run, `git status` for re-clonable repositories.
- **In-use detection**: running processes, open files, paths bind-mounted into running
  containers, models loaded in Ollama. Nothing in use is ever proposed.
- **Dormancy**: items used in the last 60 days (configurable) are classified and shown but
  not proposed; active projects raise the level of their regenerable content.
- **Script generator**: `set -euo pipefail`, `df` before and after, one commented block per
  item with the rule id and the reason, `safe` items removed, `review` items moved to the
  trash (`gio trash`), tool commands for managed stores, user block then `sudo` block.
- **Desktop app** (`cleanator`, Rust + iced): squarified treemap with drill-down coloured by
  rule category, KPI header, recommendations with two depths (one line, then the exact
  command, last use, regeneration, cost, rule and documentation link), grouping of rules
  with many items, tabs *Explorer* / *AI assistants* / *Projects*, select-safe / clear /
  copy script / save script, light and dark themes, English and French, loading screen with
  live counters.
- **CLI** (`cleanator-cli`): `rules`, `scan`, `recommend [--explain]`, `script`, with JSON
  output, for terminals, SSH sessions and scripts.
- **Packages**: `.deb`, `.rpm` and AppImage built by `packaging/build.sh`; one package ships
  both binaries, the desktop entry and the icons.

### Known limitations

- Rules only know Linux paths; macOS and Windows are out of scope.
- Dormancy of models relies on access times; a filesystem mounted `noatime`, or a tool
  that reads every file (indexers, backups), makes models look recently used — the safe
  direction. Cross-checking with modification times is planned.
- Two Claude Desktop rules rest on observation (`source = "observed"`), not on vendor
  documentation.
- The `.rpm` is built but has not been installed on a Fedora machine yet.
- No duplicate-file detection yet.

[0.1.0]: https://github.com/blackberrylabs/cleanator/releases/tag/v0.1.0
