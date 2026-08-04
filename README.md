# rasitop

<img src="docs/assets/rasitop-menu-bar.png" alt="rasitop menu bar performance monitor" width="320">

rasitop is a low-overhead macOS menu bar monitor for Apple Silicon. A Rust
sampling engine reads CPU, GPU, temperature, fan, and power telemetry; a compact
Swift/AppKit interface keeps the live view out of your way.

Requires macOS, Xcode command-line tools, Rust, and
[`just`](https://github.com/casey/just). The Nix development shell provides the
pinned Rust tools and `just`.

```sh
just          # list commands
just build    # debug build
just check    # format, lint, and test
just preview  # open the live-data preview
just install  # install the release app in /Applications
```

Inspired by [macmon](https://github.com/vladkens/macmon) and
[Stats](https://github.com/exelban/stats).
