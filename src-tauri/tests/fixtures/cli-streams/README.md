# CLI stream fixtures — 2026-10-20

Real CLI stdout from synthetic temporary projects:

- `claude-2.1.304/read-command.jsonl`: Read + Bash/pwd + reply.
- `codex-0.163.0/read-command.jsonl`: read-only file commands + pwd + reply.
- `opencode-1.19.3/read.jsonl`: read tool + reply; official temporary binary, checksum and signature verified. The pre-existing installed binary remains broken.
- `grok-1.0.62/read-command.jsonl`: actual error result; usage exhausted. This is failure evidence, not successful streaming evidence.

Paths and native session IDs are replaced. Unrelated Claude system/hook notifications are omitted; remaining protocol frame shapes and tool call IDs are preserved for pairing tests. No real project source, credentials or raw provider stderr is included.

`chat::tools::fixtures` replays these frames through the production parser. OpenCode generated error and Codex diff fallbacks have separate behavior tests. Runtime acceptance reports are in `docs/dogfood/samples/simple-chat/`.
