set shell := ["zsh", "-cu"]

app:
    cargo build --release

app-debug:
    cargo build

app-run: app
    open -n target/release/rasitop.app

app-clean:
    cargo clean
