set -euo pipefail
exec timeout --kill-after=1s 11s python3 -B /code/probe.py large-serial 5 node_modules/.pnpm/@openai+codex@0.153.4-darwin-arm64/node_modules/@openai/codex/vendor/aarch64-apple-darwin/bin/codex
