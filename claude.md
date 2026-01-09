# Claude Development Guide

This document provides guidelines for Claude Code when working on this project.

## Version Control: Jujutsu (jj)

This project uses [Jujutsu](https://github.com/martinvonz/jj) for version control, which coexists with git.

### Common jj Commands

#### Viewing Changes
```sh
# Show current changes (working copy)
jj diff

# Show changes in a specific commit
jj show @           # Current commit
jj show @-          # Parent commit
jj show <change-id> # Specific commit

# View commit history
jj log
jj log -r ::@       # Show history up to current commit
```

#### Making Commits
```sh
# Create a new commit with current changes
jj commit -m "commit message"

# Amend the current commit
jj describe -m "new description"

# Start a new change
jj new
```

#### Working with Branches
```sh
# Create a new branch
jj branch create <branch-name>

# Set/move a branch to current commit
jj branch set <branch-name>

# List branches
jj branch list
```

#### Rebasing and Editing
```sh
# Rebase current commit onto another
jj rebase -d <destination>

# Squash current commit into parent
jj squash

# Edit a specific commit
jj edit <change-id>
```

### Key Differences from Git

1. **Every change gets a unique Change ID** - This persists across rebases and amendments
2. **Working copy is always a commit** - No staging area, changes are automatically tracked
3. **Automatic rebase** - Descendants are automatically rebased when you edit history
4. **@ refers to the working copy** - Similar to HEAD in git
5. **@- is the parent** - @+ is child, @-- is grandparent, etc.

### Guidelines for Claude

When working with this repository:

1. **Check current status**: Use `jj status` or `jj diff` to see what's changed
2. **View recent changes**: Use `jj show @-` to understand recent commits
3. **Create commits**: Use `jj commit` instead of git commit
4. **Amend changes**: Use `jj describe` to update commit messages

### Useful jj Concepts

- **Change ID**: Unique identifier for a change (starts with letters, e.g., `uorzurvttv...`)
- **Commit ID**: Git-compatible SHA (changes when you amend)
- **Working Copy (@)**: The current state of your files
- **Colocated repo**: jj and git share the same working directory

## Project-Specific Guidelines

### Building and Testing
```sh
# Build the project
cargo build

# Build release
cargo build --release

# Run with cargo
cargo run -- [args]

# Run tests with cargo nextest (preferred)
cargo nextest run

# Run tests with standard cargo test (fallback)
cargo test

# Check for errors without building
cargo check

# Run clippy for lints
cargo clippy
```

### Code Style

- Follow standard Rust conventions
- Use `rustfmt` for formatting: `cargo fmt`
- Address clippy warnings: `cargo clippy`

### Common Tasks

#### Testing powermetrics parsing
```sh
# Run with test file
cargo run -- --from-file path/to/sample.plist

# Run with verbose output
cargo run -- --verbose

# Run with different formats
cargo run -- --format json
cargo run -- --format csv
cargo run -- --format human
```

#### Adding new features

1. Create a new change: `jj new`
2. Implement the feature
3. Test thoroughly with `cargo nextest run`
4. Commit: `jj commit -m "description"`
