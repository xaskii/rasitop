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
      local time_limit_millis="$4"
      shift 4

      local cargo_instruments_args=(
        --profile profiling
        --bin "$bin"
        --template "$template"
        --output "$output"
      )
      if [[ -n "$time_limit_millis" ]]; then
        cargo_instruments_args+=(--time-limit "$time_limit_millis")
      fi
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

    profile_activity() {
      local duration_seconds="$1"
      local warmup_seconds="''${ACTIVITY_WARMUP_SECONDS:-10}"
      if [[ ! "$warmup_seconds" =~ ^[0-9]+$ ]]; then
        echo "ACTIVITY_WARMUP_SECONDS must be a non-negative integer" >&2
        exit 2
      fi
      if (( duration_seconds < 2 )); then
        echo "Activity Monitor measurements require at least 2 seconds" >&2
        exit 2
      fi

      local run_id="$timestamp-$$"
      local trace="''${ACTIVITY_TRACE_OUTPUT:-$output_dir/rasitop-app-activity-$run_id.trace}"
      local export_xml="''${ACTIVITY_XML_OUTPUT:-$output_dir/rasitop-app-activity-$run_id.xml}"
      local summary="''${ACTIVITY_SUMMARY_OUTPUT:-$output_dir/rasitop-app-activity-$run_id.json}"
      local app_log="''${ACTIVITY_APP_LOG:-$output_dir/rasitop-app-activity-$run_id.log}"
      local app_executable="$repo_root/target/release/rasitop.app/Contents/MacOS/rasitop"
      local summary_executable="$repo_root/target/release/rasitop-activity-summary"
      local app_lifetime_seconds=$((warmup_seconds + duration_seconds + 30))

      for path in "$trace" "$export_xml" "$summary" "$app_log"; do
        mkdir -p "$(dirname "$path")"
        if [[ -e "$path" ]]; then
          echo "refusing to overwrite existing activity output: $path" >&2
          exit 1
        fi
      done

      cargo build \
        --release \
        --target-dir "$repo_root/target" \
        --bin rasitop-app \
        --bin rasitop-activity-summary >&2
      if [[ ! -x "$app_executable" || ! -x "$summary_executable" ]]; then
        echo "release app or Activity Monitor summarizer was not built" >&2
        exit 1
      fi

      activity_app_pid=""
      activity_summary_tmp="$summary.tmp.$$"
      cleanup_activity() {
        if [[ -n "''${activity_summary_tmp:-}" ]]; then
          rm -f "$activity_summary_tmp"
        fi
        if [[ -n "''${activity_app_pid:-}" ]] && kill -0 "$activity_app_pid" 2>/dev/null; then
          kill -TERM "$activity_app_pid" 2>/dev/null || true
          wait "$activity_app_pid" 2>/dev/null || true
        fi
        activity_app_pid=""
      }
      trap cleanup_activity EXIT

      "$app_executable" \
        --profile-duration-seconds "$app_lifetime_seconds" \
        >"$app_log" 2>&1 &
      activity_app_pid=$!
      sleep "$warmup_seconds"
      if ! kill -0 "$activity_app_pid" 2>/dev/null; then
        wait "$activity_app_pid" || true
        echo "release app exited during warmup; log: $app_log" >&2
        exit 1
      fi

      xcrun xctrace record \
        --template "Activity Monitor" \
        --attach "$activity_app_pid" \
        --time-limit "''${duration_seconds}s" \
        --output "$trace" \
        --no-prompt >&2
      if [[ ! -d "$trace" ]]; then
        echo "Activity Monitor did not produce a trace at $trace" >&2
        exit 1
      fi

      xcrun xctrace export \
        --input "$trace" \
        --xpath '/trace-toc/run[@number="1"]/data/table[@schema="activity-monitor-process-live"]' \
        --output "$export_xml" >&2
      if [[ ! -s "$export_xml" ]]; then
        echo "Activity Monitor export is empty: $export_xml" >&2
        exit 1
      fi

      "$summary_executable" "$export_xml" >"$activity_summary_tmp"
      mv "$activity_summary_tmp" "$summary"
      activity_summary_tmp=""

      cleanup_activity
      trap - EXIT
      echo "trace: $trace" >&2
      echo "export: $export_xml" >&2
      echo "summary: $summary" >&2
      cat "$summary"
    }

    if [[ "''${1:-}" == app ]]; then
      mode="''${2:-all}"
      duration_seconds="''${3:-60}"
      watchdog_grace_seconds="''${INSTRUMENTS_WATCHDOG_GRACE_SECONDS:-10}"
      if (( $# > 3 )) \
        || [[ ! "$duration_seconds" =~ ^[1-9][0-9]*$ ]] \
        || [[ ! "$watchdog_grace_seconds" =~ ^[0-9]+$ ]]; then
        echo "usage: nix run .#profile -- app [cpu|allocations|activity|all] [seconds]" >&2
        exit 2
      fi
      time_limit_millis="$((
        (duration_seconds + watchdog_grace_seconds) * 1000
      ))"
      case "$mode" in
        cpu)
          output="''${INSTRUMENTS_OUTPUT:-$output_dir/rasitop-app-cpu-$timestamp.trace}"
          profile_target \
            rasitop-app \
            "CPU Profiler" \
            "$output" \
            "$time_limit_millis" \
            --profile-duration-seconds "$duration_seconds"
          ;;
        allocations)
          output="''${INSTRUMENTS_OUTPUT:-$output_dir/rasitop-app-allocations-$timestamp.trace}"
          profile_target \
            rasitop-app \
            Allocations \
            "$output" \
            "$time_limit_millis" \
            --profile-duration-seconds "$duration_seconds"
          ;;
        activity)
          profile_activity "$duration_seconds"
          ;;
        all)
          profile_target \
            rasitop-app \
            "CPU Profiler" \
            "$output_dir/rasitop-app-cpu-$timestamp.trace" \
            "$time_limit_millis" \
            --profile-duration-seconds "$duration_seconds"
          profile_target \
            rasitop-app \
            Allocations \
            "$output_dir/rasitop-app-allocations-$timestamp.trace" \
            "$time_limit_millis" \
            --profile-duration-seconds "$duration_seconds"
          ;;
        *)
          echo "usage: nix run .#profile -- app [cpu|allocations|activity|all] [seconds]" >&2
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
    time_limit_millis="''${INSTRUMENTS_TIME_LIMIT_MS:-}"
    if [[ -n "$time_limit_millis" && ! "$time_limit_millis" =~ ^[1-9][0-9]*$ ]]; then
      echo "INSTRUMENTS_TIME_LIMIT_MS must be a positive integer" >&2
      exit 2
    fi
    profile_target rasitop "$template" "$output" "$time_limit_millis" "$@"
  '';
}
