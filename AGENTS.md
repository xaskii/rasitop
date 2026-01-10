# AGENTS.md

## Repo Notes

- Version control uses Jujutsu (`jj`) alongside git; prefer `jj status`, `jj diff`, and `jj commit`.
- `powermetrics` cluster names vary across machines (e.g., `E-Cluster`, `E0-Cluster`, `P-Cluster`, `P0-Cluster`, `P1-Cluster`).
- rasitop aggregates E/P cluster metrics by taking the **max** `freq_hz` and max busy ratio across matching clusters.
- `fixtures/powermetrics.xml` is a lightweight fixture; use a fresh `powermetrics -f plist` capture when updating parser expectations.

## Jujutsu Workflow

- Check changes with `jj status` or `jj diff`.
- View history with `jj log` or `jj show @`.
- Start new work with `jj new`; amend message with `jj describe -m`.
- The working copy is always a commit (`@`), and `@-` refers to the parent.

## Build and Test

- Build with `cargo build` (use `cargo build --release` for release).
- Run tests with `cargo nextest run` (fallback: `cargo test`).
- Format and lint with `cargo fmt` and `cargo clippy`.
- Validate parsing with `cargo run -- --from-file path/to/sample.plist`.

## Runtime Checks

- Try `cargo run -- --from-file fixtures/powermetrics.xml --format human` to sanity-check parsing.
- Live sampling requires `sudo` (powermetrics); stop with Ctrl-C to verify shutdown.
