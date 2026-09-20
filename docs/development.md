# 开发指南

本文件供编写计划、调整仓库指引或组织验证时查阅。产品范围见 [requirements.md](requirements.md)，检查选择见 [testing.md](testing.md#开发变更验证矩阵)。

## 计划规范

非平凡功能、重构或迁移使用正式计划；小型、局部、可逆修复可以直接实施。用户明确要求计划时按要求执行。

- 路径：`docs/plans/YYYY-MM-DD/HH:mm-<topic>.md`，使用当前本地日期、时间；MVP 默认主题为 `mvp`，具体任务使用具体主题。
- 默认中文。包含目标、非目标、成功标准、当前状态、里程碑、具体任务、风险、待确认问题和验证策略；任务说明依赖与可验证产出，能独立执行和 Review。
- 记录状态和代码基线；将已实现、待实现、历史证据分开，完成后回填验证结果。没有阻塞问题可直接写“无”，不为填模板制造确认步骤。
- 新建或更新计划时，同轮更新 [PLANS.md](PLANS.md) 对应条目的摘要或排序。
- 索引用 `## YYYY-MM-DD` 分隔，日期倒序、每天文件名时间倒序；列表格式为文件名和一句话引用摘要。当前入口通过摘要明确，不用打乱时间排序置顶。
- 模板约束信息完整性，不强制所有内容使用表格；简单并行信息用列表即可。

## 指引与提示词维护

- AGENTS 保留稳定边界与文档路由；具体流程放在相应指南。新增规则说明适用条件，避免“所有任务都先……”式前置要求。
- 需求定义产品范围，契约区分当前实现和目标，计划定义实施顺序，dogfood 记录有基线和时间的证据。变更规范时更新对应入口，历史验收不当作当前完成证明。
- 不在仓库固定某个模型或复制整个模型提示指南；Loom 支持多个 CLI。模型特例归适配器能力和版本验证。
- 技能只在具体工作流确有帮助时使用；同名全局技能去重和个人语言偏好由用户另行维护，不写入项目规则。
- Review 的 JSON、IPC 信封、Todo 解析等机器契约保持稳定；自然语言说明默认中文，可按用户语言要求调整。
- 提示词改动用 [行为验收案例](guides/agent-behavior-evaluation.md) 对照。离线测试验证传输和约束；真实模型是否减少停顿或成本需独立观测。

## 预览约定

启动前执行 `scripts/debug.sh stop`；桌面用 `scripts/debug.sh desktop`，仅 Web 用 `scripts/debug.sh web`。完成后执行 `scripts/debug.sh stop`，清理本仓库的 Vite/Tauri dev 进程。

固定端口 `1420`；Web 辅助脚本 `scripts/preview.sh` 的日志为 `/tmp/loom-preview-vite.log`，PID 为 `/tmp/loom-preview-vite.pid`。手工启动先确认无旧进程，再使用 `pnpm dev --host 127.0.0.1 --port 1420 --strictPort`。不另开端口绕过残留进程；排障细节见 [scripts/README.md](../scripts/README.md)。

## 官方参考

- [GPT-6 Astra 提示最佳实践](https://developers.openai.com/api/docs/guides/latest-model?model=gpt-6-astra)
- [Skills、AGENTS 与任务边界](https://developers.openai.com/blog/rethinking-skills-and-prompts-for-gpt-6-astra#better-skills)

参考原则为按需加载、明确完成条件、按改动校准验证；本仓库具体产品和权限规则以现行需求与契约为准。
