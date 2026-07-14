#!/bin/bash

set -euo pipefail

ARGS=("$@")
xcrun clang "${ARGS[@]}"

if [[ "${CARGO_BIN_NAME:-}" != "rasitop-app" ]]; then
  exit 0
fi

ARTIFACT=""
for ((index = 0; index < ${#ARGS[@]}; index++)); do
  if [[ "${ARGS[index]}" == "-o" ]] && ((index + 1 < ${#ARGS[@]})); then
    ARTIFACT="${ARGS[index + 1]}"
    break
  fi
done

if [[ -z "$ARTIFACT" || ! -x "$ARTIFACT" ]]; then
  exit 0
fi

PROFILE_DIR="$(dirname "$(dirname "$ARTIFACT")")"
APP_DIR="$PROFILE_DIR/rasitop.app"
APP_EXECUTABLE="$APP_DIR/Contents/MacOS/rasitop"

mkdir -p "$(dirname "$APP_EXECUTABLE")"
cp "$ARTIFACT" "$APP_EXECUTABLE"
codesign --force --sign - --timestamp=none "$APP_DIR"
