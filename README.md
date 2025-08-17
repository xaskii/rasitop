# Rasitop

Intended to be a rewrite of asitop in Rust, but not sure what direction I'm gonna go with this.

## Development Setup

```sh
# Get a rust toolchain somehow
cargo build --bin rasitop (only guaranteed to build on stable)
```

## TODO for implementation
- [x] boilerplate with clap and tokio stuff
- [x] powermetrics inside tokio task outputting through reader and onto screen
- [x] start powermetrics parser, showing CPU and GPU usage, with power usage values for both.
- [ ] better ctrl-c handling
  - it doesn't stop everything properly. I need to listen for ctrl-c and then propogate it across the other threads
- [ ] redirect stderr to a log file
  - I can just use a tmpdir for now like idk. I also have to figure out why it keeps saying underflow occured. 
- [ ] make a crate that's responsible for the entire powermetrics plist schema
  - This is a more platform agnostic solution to use the binary instead of trying to the objc bindings. Regardless I just want to have a struct I use separate from the schema, and then I can substitute out that provider when I want to switch to objc bindings later.
