# Loom 会话导出

## 标题

```text
中文导出 / 安全围栏
```

- 会话：chat-contract
- 快照序号：1
- 消息数：2
- 运行中快照：false

session.json 保留完整结构化快照；manifest.json 记录日志边界与缺失项。请备份整个导出目录。旧版 Loom 不会自动导入此 JSON。

## 消息 1 · user · complete

`````text
原始文本
```
<script>not executable</script>
````
中文与尾部空白  
`````

## 消息 2 · assistant · complete

```text
world
```

## 回合执行记录

```json
{
  "id": "turn-contract",
  "clientRequestId": "request-contract",
  "userMessageId": "user-contract",
  "assistantMessageId": "assistant-contract",
  "invocation": {
    "agentId": "agent-contract",
    "adapterType": "grok_cli",
    "program": "grok",
    "args": [
      "-p",
      "[PROMPT]"
    ],
    "cwd": "/project",
    "permissionMode": "explore",
    "stdinPrompt": false,
    "outputMode": "streaming_json",
    "configFingerprint": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
  },
  "status": "completed",
  "acceptedAtMs": 1,
  "startedAtMs": 2,
  "finishedAtMs": 3,
  "processId": 123,
  "exitCode": 0,
  "terminationReason": null,
  "errorSummary": null,
  "stdoutLogRef": "runs/turn-contract/stdout.log",
  "stderrLogRef": "runs/turn-contract/stderr.log"
}
```

- [stdout](runs/turn-contract/stdout.log)（39 bytes）
- [stderr](runs/turn-contract/stderr.log)（16 bytes）

