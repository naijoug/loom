# Loom

Loom 是一个面向软件开发任务的桌面端编程助手工作台。它用于统一调度用户本机已安装的多种 Agent 工具，例如 Codex、Claude Code、OpenClaw、Hermes 等，并把需求讨论、计划完善、实施、Review、调试验收和自动修复组织成一个连续流程。

## 核心流程

1. 需求讨论  
   多个 Agent 基于同一需求生成各自的计划、风险分析和测试建议，系统汇总共识与冲突，形成最终实施计划。

2. 实施与 Review  
   用户选择一个主 Agent 负责实现，其它 Agent 参与 Review、补充测试建议和发现风险。

3. 调试验收  
   系统启动本地开发或验证命令，实时展示日志，自动识别错误。用户也可以在人工介入模式下补充问题、截图、日志和复现步骤。

4. 修复循环  
   当验收失败时，系统将错误摘要、日志和上下文交给主 Agent 修复，再继续运行验证，直到任务完成、用户停止或出现明确阻塞。

## 目标能力

- 管理多种本地 Agent 工具。
- 为不同阶段选择不同 Agent 角色。
- 并行生成和完善计划文档。
- 执行本地命令并实时展示日志。
- 自动分析测试失败、启动失败和运行时错误。
- 支持自动修复与人工介入两种调试方式。
- 保存任务全过程记录和最终验收总结。

## 文档

- [需求文档](docs/requirements.md)
- [架构](docs/architecture.md)
- [Agent Adapter](docs/agent-adapter.md)
- [任务状态机](docs/task-state-machine.md)
- [安全策略](docs/security-policy.md)
- [完整版本需求审计](docs/requirements-audit.md)
- [完整版本验收报告](docs/dogfood/complete-version-2026-07-24.md)
- [用户测试指南](docs/testing.md)
- [实施计划索引](docs/PLANS.md)

## MVP 范围

第一版目标是跑通完整开发闭环：

- 打开本地项目。
- 配置 Agent 命令。
- 多 Agent 讨论并生成最终计划。
- 选择主 Agent 实施。
- 使用协作 Agent Review。
- 启动本地调试或验证命令。
- 实时查看日志。
- 根据错误自动修复。
- 支持用户人工反馈问题。
- 生成任务总结。

## 建议架构

项目可以拆分为以下核心模块：

- `UI`：桌面端界面、任务流程、日志面板、配置面板。
- `Orchestrator`：任务状态机、多 Agent 调度、阶段推进。
- `Agent Adapter`：适配 Codex、Claude Code、OpenClaw、Hermes 等工具。
- `Command Runner`：本地命令执行、日志采集、进程管理。
- `Project Analyzer`：识别项目类型、脚本和验证命令。
- `Review Engine`：组织协作 Agent Review。
- `Persistence`：保存任务、日志、计划和配置。

## 当前状态

仓库已实现第一版完整本地闭环：多 Agent 规划与互评、主 Agent Todo 实施、独立实施 Review 与 blocker 门禁、受策略保护的命令/PTY、自动与人工调试修复循环、结构化附件证据、重启恢复，以及基于 Git baseline 的 JSON/Markdown 交付总结。dummy Agent 仅用于测试 fixture，不会作为真实任务 Agent 展示。

本地完整门禁：

```bash
pnpm check
```

启动桌面端与测试步骤见 [用户测试指南](docs/testing.md)。
