# AGENTS.md

## Repo Notes

- Version control uses Jujutsu (`jj`) alongside git; prefer `jj status`, `jj diff`, and `jj commit`.
- rasitop aggregates per-core E/P metrics by taking the **max** busy ratio and max frequency across cores.

## Jujutsu Workflow

- Check changes with `jj status` or `jj diff`.
- View history with `jj log` or `jj show @`.
- Start new work with `jj new`; amend message with `jj describe -m`.
- The working copy is always a commit (`@`), and `@-` refers to the parent.
- Prefer the squash workflow, where you describe the work you're going to do, make a new commit, then squash work into it as you go. `jj squash -t <ref>`

## Build and Test

- Build with `cargo build` (use `cargo build --release` for release).
- Run tests with `cargo nextest run` (fallback: `cargo test`).
- Format and lint with `cargo fmt` and `cargo clippy`.
- Validate parsing with `cargo run -- --from-file path/to/sample.plist`.

## Runtime Checks

- Try `cargo run -- --format human` to sanity-check live sampling output.
- Stop with Ctrl-C to verify clean shutdown.
