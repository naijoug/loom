# Dogfood / 验收清单 — 10:15 local agent chat rebuild

- Branch: `feat/local-agent-chat-rebuild-1015`
- Date: 2026-09-17

## 自动化
- [ ] `pnpm test`
- [ ] `pnpm exec tsc -p tsconfig.json --noEmit`
- [ ] `cd src-tauri && cargo test chat::`

## 桌面（需本机 grok 已登录）
- [ ] 选择项目 → 新建会话 → 发送一轮流式回复
- [ ] explore / ask（确认门）/ auto 权限可切换且行为符合预期
- [ ] 停止生成后可再发
- [ ] 重命名、搜索、归档、恢复
- [ ] 重启应用后会话仍在；中断回合不为永久 streaming
- [ ] 切换会话时草稿隔离、不会串流

未登录/未安装 CLI：记 **blocked**，不要用 mock 填 passed。
