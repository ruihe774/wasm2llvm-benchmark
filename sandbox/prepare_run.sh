#!/bin/bash
# Prepare a fresh agent workspace and its sandbox settings.
#
#   sandbox/prepare_run.sh RUN_NAME [RUNS_ROOT]
#
# Creates RUNS_ROOT/RUN_NAME (default RUNS_ROOT: /mnt/workspace/w2l-runs) as a
# copy of agent/, and writes sandbox/generated/RUN_NAME.settings.json (a file
# Claude Code never reads on its own; it is passed with --settings).
set -euo pipefail
W2L="$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)"
NAME="${1:?usage: prepare_run.sh RUN_NAME [RUNS_ROOT]}"
ROOT="${2:-/mnt/workspace/w2l-runs}"
RUN="$ROOT/$NAME"
case "$RUN" in "$W2L"/*) echo "RUNS_ROOT must be outside $W2L" >&2; exit 1;; esac
[ -e "$RUN" ] && { echo "$RUN already exists" >&2; exit 1; }
mkdir -p "$ROOT"
cp -R --no-preserve=all "$W2L/agent" "$RUN"
find "$RUN" -xdev -name __pycache__ -prune -exec rm -rf {} +
mkdir -p "$RUN/.tmp" "$RUN/.cargo-home"
git -C "$RUN" init -q && git -C "$RUN" add -A && \
  git -C "$RUN" -c user.name=w2l -c user.email=w2l@localhost commit -qm "initial workspace"
mkdir -p "$W2L/sandbox/generated"
OUT="$W2L/sandbox/generated/$NAME.settings.json"
sed -e "s|@RUN@|$RUN|g" -e "s|@W2L@|$W2L|g" -e "s|@HOME@|$HOME|g" "$W2L/sandbox/settings.template.json" > "$OUT"
python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$OUT"
cat <<EOF
Workspace: $RUN
Settings:  $OUT

Headless run:
  cd $RUN && claude -p "\$(cat PROMPT.md)" \\
    --settings $OUT --setting-sources project --safe-mode --strict-mcp-config \\
    --permission-mode dontAsk --output-format stream-json --verbose \\
    > $ROOT/$NAME.transcript.jsonl 2> $ROOT/$NAME.stderr.log

Interactive run (paste the contents of PROMPT.md as the first message):
  cd $RUN && claude --settings $OUT --setting-sources project --safe-mode --strict-mcp-config --permission-mode dontAsk

Grade afterwards:
  python3 $W2L/verification/verify.py $RUN --out $ROOT/$NAME-report
EOF
