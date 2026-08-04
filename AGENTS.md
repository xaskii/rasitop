# AGENTS.md

Version control uses Jujutsu (`jj`) alongside git; prefer `jj status`, `jj diff`,
and `jj commit`.

## Jujutsu Workflow

- Check changes with `jj status` or `jj diff`.
- View history with `jj log` or `jj show @`.
- Start new work with `jj new`; amend message with `jj describe -m`.
- The working copy is always a commit (`@`), and `@-` refers to the parent.
- Prefer the squash workflow, where you describe the work you're going to do, make a new commit, then squash work into it as you go. `jj squash -t <ref>`

## Build and Test

- Build with `just build` (use `just release` for release).
- Format, lint, and test with `just check`.
- Raw Cargo commands remain supported when a narrower check is useful.
- The pinned nightly toolchain enables the experimental `-Zpolonius=next`
  borrow checker for all Cargo builds.
- Validate the unsafe FFI boundary with
  `cargo miri test --lib ffi::tests`.

## Runtime Checks

- Try `cargo run --release -- record --duration 1m` to sanity-check live CPU
  sampling output.
- Stop with Ctrl-C to verify clean shutdown.
- For UI work, open a timed release preview with `just preview <seconds>`.
- The preview hosts the production sensor summary in a standard window, but it
  does not exercise native `NSMenu` items or dismissal. Smoke-test the normal
  status menu after changing menu integration or behavior.
