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
    timestamp="$(date +%Y%m%d-%H%M%S)"
    output_dir="''${INSTRUMENTS_OUTPUT_DIR:-$repo_root/target/profiling}"

    # Instruments is provided by Xcode, while Rust and cargo-instruments come
    # from this Nix closure.
    export PATH="/usr/bin:/bin:/usr/sbin:/sbin:$PATH"

    rust_src="$(rustc --print sysroot)/lib/rustlib/src/rust/library/std/Cargo.toml"
    if [[ ! -f "$rust_src" ]]; then
      echo "the Rust toolchain must include rust-src" >&2
      exit 1
    fi

    profile_target() {
      local bin="$1"
      local template="$2"
      local output="$3"
      shift 3

      local cargo_instruments_args=(
        --profile profiling
        --bin "$bin"
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
    }

    if [[ "''${1:-}" == app ]]; then
      mode="''${2:-all}"
      duration_seconds="''${3:-60}"
      if (( $# > 3 )) || [[ ! "$duration_seconds" =~ ^[1-9][0-9]*$ ]]; then
        echo "usage: nix run .#profile -- app [cpu|allocations|all] [seconds]" >&2
        exit 2
      fi
      case "$mode" in
        cpu)
          output="''${INSTRUMENTS_OUTPUT:-$output_dir/rasitop-app-cpu-$timestamp.trace}"
          profile_target \
            rasitop-app \
            "CPU Profiler" \
            "$output" \
            --profile-duration-seconds "$duration_seconds"
          ;;
        allocations)
          output="''${INSTRUMENTS_OUTPUT:-$output_dir/rasitop-app-allocations-$timestamp.trace}"
          profile_target \
            rasitop-app \
            Allocations \
            "$output" \
            --profile-duration-seconds "$duration_seconds"
          ;;
        all)
          profile_target \
            rasitop-app \
            "CPU Profiler" \
            "$output_dir/rasitop-app-cpu-$timestamp.trace" \
            --profile-duration-seconds "$duration_seconds"
          profile_target \
            rasitop-app \
            Allocations \
            "$output_dir/rasitop-app-allocations-$timestamp.trace" \
            --profile-duration-seconds "$duration_seconds"
          ;;
        *)
          echo "usage: nix run .#profile -- app [cpu|allocations|all] [seconds]" >&2
          exit 2
          ;;
      esac
      exit 0
    fi

    if (( $# == 0 )); then
      set -- record --interval 1ms --duration 10s
    fi
    template="''${INSTRUMENTS_TEMPLATE:-CPU Profiler}"
    output="''${INSTRUMENTS_OUTPUT:-$output_dir/rasitop-cpu-$timestamp.trace}"
    profile_target rasitop "$template" "$output" "$@"
  '';
}
