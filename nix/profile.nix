{
  cargoInstruments,
  rustToolchain,
  writeShellApplication,
}:

writeShellApplication {
  name = "rasitop-profile";
  runtimeInputs = [
    cargoInstruments
    rustToolchain
  ];
  text = ''
    if [[ ! -f Cargo.toml ]]; then
      echo "run this command from the rasitop repository root" >&2
      exit 1
    fi

    repo_root="$PWD"
    template="''${INSTRUMENTS_TEMPLATE:-CPU Profiler}"
    timestamp="$(date +%Y%m%d-%H%M%S)"
    output="''${INSTRUMENTS_OUTPUT:-$repo_root/target/profiling/rasitop-cpu-$timestamp.trace}"

    # Instruments is provided by Xcode, while Rust and cargo-instruments come
    # from this Nix closure.
    export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"

    if (( $# == 0 )); then
      set -- record --interval 1ms --duration 10s
    fi

    rust_src="$(rustc --print sysroot)/lib/rustlib/src/rust/library/std/Cargo.toml"
    if [[ ! -f "$rust_src" ]]; then
      echo "the Rust toolchain must include rust-src" >&2
      exit 1
    fi

    cargo_instruments_args=(
      --profile profiling
      --bin rasitop
      --template "$template"
      --output "$output"
    )
    if [[ "''${INSTRUMENTS_NO_OPEN:-0}" == 1 ]]; then
      cargo_instruments_args+=(--no-open)
    fi

    mkdir -p "$(dirname "$output")"

    RUSTC_BOOTSTRAP=1 \
      CARGO_UNSTABLE_BUILD_STD=std \
      RUSTFLAGS="''${RUSTFLAGS:+$RUSTFLAGS }-C force-frame-pointers=yes" \
      cargo instruments \
        "''${cargo_instruments_args[@]}" \
        -- "$@"

    echo "trace: $output"
    if [[ "''${INSTRUMENTS_NO_OPEN:-0}" == 1 ]]; then
      echo "open with: open '$output'"
    fi
  '';
}
