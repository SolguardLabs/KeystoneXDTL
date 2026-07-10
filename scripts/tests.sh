#!/usr/bin/env bash
set -euo pipefail

if ! command -v cargo >/dev/null 2>&1 && [ -f "$HOME/.cargo/env" ]; then
  # shellcheck source=/dev/null
  . "$HOME/.cargo/env"
fi

NODE_BIN="${NODE_BIN:-node}"
if ! command -v "$NODE_BIN" >/dev/null 2>&1; then
  candidates=()
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
      candidates=("/c/Program Files/nodejs/node.exe")
      ;;
    *)
      ;;
  esac
  for candidate in "${candidates[@]}"; do
    if [ -x "$candidate" ] && "$candidate" --version >/dev/null 2>&1; then
      NODE_BIN="$candidate"
      break
    fi
  done
fi
if ! command -v "$NODE_BIN" >/dev/null 2>&1; then
  echo "node not found in PATH; install Node.js 24+ or set NODE_BIN" >&2
  exit 127
fi

cargo test --locked
"$NODE_BIN" --test "tests/node/*.test.js"
