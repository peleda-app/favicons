#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="${FAVICONS_BIN:-${CARGO_TARGET_DIR:-target}/release/favicons}"
[ -x "$BIN" ] || cargo build --release --locked
mkdir -p loadtest/out

FAVICONS_LOG="${FAVICONS_LOG:-warn}" "$BIN" --config loadtest/config.toml &
PID=$!
trap 'kill "$PID" 2>/dev/null || true' EXIT
for _ in $(seq 1 100); do
  curl -fs http://127.0.0.1:8080/healthz >/dev/null && break
  sleep 0.2
done

if command -v k6 >/dev/null; then
  k6 run -e BASE_URL=http://127.0.0.1:8080 -e OUT_DIR=loadtest/out "$@" loadtest/favicons.js
else
  if [ "$(uname)" = "Linux" ]; then net=(--network host); base=http://127.0.0.1:8080; else net=(); base=http://host.docker.internal:8080; fi
  docker run --rm -i ${net[@]+"${net[@]}"} -u "$(id -u):$(id -g)" -v "$PWD/loadtest:/loadtest" \
    -e BASE_URL="$base" -e OUT_DIR=/loadtest/out "${K6_IMAGE:-grafana/k6:1.3.0}" run "$@" /loadtest/favicons.js
fi
