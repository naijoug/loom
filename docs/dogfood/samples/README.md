# 离线导出样本

[chat-export/transcript.md](chat-export/transcript.md)、[session.json](chat-export/session.json) 和 [manifest.json](chat-export/manifest.json) 由生产 Chat 导出器生成，日志副本位于同目录的 `runs/`。

内容来自契约 fixture，包含合成的标题、进程号、时间、正文/代码围栏和已脱敏日志。**它不是任何真实 Agent 的执行证明。** JSON 内的项目路径是生成时的临时测试目录；这些绝对路径仅作历史数据，读取样本不需要该目录。相对日志路径可随文件夹复制。

重现（只生成本地文件，不调用 CLI Agent 或网络）：

```bash
CARGO_NET_OFFLINE=true LOOM_EXPORT_SAMPLE=1 cargo test --manifest-path src-tauri/Cargo.toml \
  chat::export::tests::generate_rollback_sample -- --exact --ignored --nocapture
```

命令输出 `EXPORT_SAMPLE=<临时导出目录>`。该目录由生产 `export_session` 生成，可检查后整体复制到备份位置。仓库中的五个文件与本次输出逐字节一致，未修改正文或 manifest：

| 文件 | SHA-256 |
|---|---|
| session.json | `33a65c5519cd04f033f2e0a815db8c4aebbf5e983afd6768605e5dfae4df30bb` |
| transcript.md | `31bfcf8c0cc0809b5c7ffd749f075189f6fa7bf125fb80a9ee4645098f59b05c` |
| manifest.json | `0ef6196927e73711ad03f64071a2ade865f5291ed0c1b6ef8d435a08150f4f0e` |
| runs/turn-contract/stdout.log | `2e5a88b7dbfa8bd8eee3ca6f4110cd4011bdeca95305d37bf837e344ab4d4ccd` |
| runs/turn-contract/stderr.log | `49881153d7e5eb572578491921210c37477264261fc6d3db08ee150063d5023d` |
