# Contributing to Cleanator

Thanks for looking. Most contributions are **rules**: Cleanator only knows what its
`rules/*.toml` files say, and every developer's disk has something they don't cover yet.

## Ground rules (they shape every change)

1. **Cleanator never deletes.** It writes a script the user reads and runs. A change that
   makes the app remove, move or modify a file itself will not be merged.
2. **Unknown is `never`.** No rule, no proposal. Don't widen a rule to "probably cache".
3. **Managed stores are cleaned by their own tool** (`ollama rm`, `docker image prune`,
   `uv cache clean`, `snap remove --revision`…), never by `rm` inside a store with an index.
4. **No network, metadata only.** No telemetry, no update check, no reading of file
   contents. Paths, names, sizes and dates are all a rule may use.
5. **English in code, comments, commits, rules and docs.** The interface has `en` and
   `fr`; more languages welcome (see below).

## Adding or changing a rule

Read [`docs/rules.md`](docs/rules.md) first: it defines the schema, the three levels, the
detection kinds and the in-use checks. Then:

1. Pick the file by subject (`package-caches.toml`, `models.toml`, `assistants.toml`…).
   Order matters inside a file: the first rule to classify a path keeps it.
2. Write the rule with a kebab-case `id`, a `level` that matches the cleanup kind
   (`safe` → `rm` or a tool command, `review` → `trash` or a tool command, `never` → `none`),
   and an `explain` block a developer can act on: *why* it takes space, *how it
   regenerates*, *what it costs* to regenerate.
3. **Cite your source.** `source.url` is the vendor's documentation page, `read_on` the date
   you read it, `quote` the sentence that supports the rule. If nothing public documents the
   layout, use `url = "observed"` and say in `quote` what you saw, on what system, when.
   `to-verify` is for work in progress only; a release ships with none.
4. Prefer the tool's own cleanup command when one exists, and say what it needs
   (`requires = "user" | "sudo" | "docker-group-or-sudo"`).
5. Check and test:

   ```sh
   cargo test
   cargo run -p cleanator-cli -- rules            # validates ids, levels, sources
   cargo run -p cleanator-cli -- recommend ~ --explain
   ```

   `rules` must end with `0 rules still have a source to verify`.

A good pull request for a rule says what it found on your machine (sizes, paths, which
tool created it) and links the documentation you read. One rule per PR is easiest to review.

## The tower

Cleanator is a module of the [Blackberrylabs](https://github.com/blackberrylabs/blackberrylabs)
control tower: `cleanator-cli report` sends one finding per rule group, with a severity
decided by what the group frees (`severity_of` in `crates/core/src/report.rs`); a change
there is written in the CHANGELOG.

## Adding a language

Copy `locales/en.json` to `locales/<lang>.json`, translate the values (keep the `{placeholders}`),
then in `crates/app/src/i18n.rs` add a variant to `Lang`, an `include_str!` for the file,
its code in `Lang::from_code`/`code`, its detection in `Lang::from_env` and its case in
`Strings::load`, then a button in the language picker in `main.rs` (`view_lang_picker`). The test `both_locales_have_the_same_keys` will tell
you if a key is missing. Rule explanations
(`why`, `regenerate`, `cost`, `notes`) stay English on purpose: they are a single source
read by every language. Please have a native speaker read your file before opening the PR.

## Code

- Rust, stable toolchain. `cargo fmt` and `cargo clippy --all-targets` clean before pushing.
- `crates/core` has no UI dependency and must stay usable from the CLI alone.
- The desktop app is [iced](https://iced.rs) 0.13; keep widgets simple, no custom shaders.
- Tests use fixture trees under a temporary directory (`tempfile`), never the real home.

## Reporting a wrong recommendation

Open an issue with the rule `id` (shown in the app under each item and by
`cleanator-cli recommend --explain`), the path it matched, and why it should not have.
A rule that proposes something it shouldn't is a bug of the highest priority.

## License

By contributing you agree that your contribution is licensed under the MIT license of the
project.
