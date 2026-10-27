# Run records

Raw Claude Code artifacts for the wasm2llvm v2 benchmark runs, kept alongside
`reports/` and `submissions/`.

| directory | model | effort | compact window | report |
|---|---|---|---|---|
| `runv2_dsf_1017_max` | DeepSeek-V4.1-Flash-1017 | max | 786432 | `reports/runv2_dsf_1017_max/` |
| `runv2_gpt6_luna_max` | GPT-6 Luna | max | 300000 | `reports/runv2_gpt6_luna_max/` |
| `runv2_gpt6_1_terra_xhigh` | GPT-6.1 Terra | xhigh | 300000 | `reports/runv2_gpt6_1_terra_xhigh/` |

Each directory contains:

* `settings.json` — the generated Claude Code settings used for the run
  (sandbox, model and environment). `ANTHROPIC_AUTH_TOKEN` is replaced by
  `REDACTED`; no other credentials were present.
* `transcript/` — the session JSONL as saved under
  `~/.claude/projects/`, including subagent transcripts and persisted tool
  results where present. The main transcript is the `<session-uuid>.jsonl`
  file at its root.

The transcripts are the agent sessions themselves and predate verification;
see `reports/` for the scores. Redaction was verified by scanning the copied
tree for the original token values and for `sk-`-prefixed key patterns.
