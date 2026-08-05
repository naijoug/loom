# Loom 本地性能基线（2026-08-05）

## 目的

为稳定化版本建立可复跑的本地预算，避免在没有指标证据时做性能重写。测量脚本为 `pnpm bench:local`；任一指标超出预算会以非零状态退出。

## 环境

- macOS 26.5.2 (25F84), arm64
- Node.js v22.22.3
- pnpm 10.29.2
- rustc 1.95.0
- 分支：`codex/project-stabilization`

## 结果

| 指标 | 实测 | 预算 | 结果 |
|---|---:|---:|---|
| 本地预览冷启动并返回首个页面 | 413.77 ms | 5,000 ms | 通过 |
| 当前 Loom 仓库项目扫描（10 次均值） | 19.64 ms | 250 ms | 通过 |
| reducer 接收 10,000 行命令日志 | 12.00 ms | 500 ms | 通过 |
| 5,000 条长任务时间线建模 | 3.28 ms | 200 ms | 通过 |
| 服务端渲染 1,000 条时间线 | 41.64 ms | 500 ms | 通过 |
| benchmark Node 进程 RSS | 92.36 MiB | 256 MiB | 通过 |

长任务 fixture 创建及渲染阶段的 RSS 增量为 14.70 MiB，仅作为趋势数据记录，不单独设置门禁。

## 测量边界

- 冷启动测量的是固定端口 `1420` 上从无预览进程到 Vite 页面完整返回的时间；脚本开始前和结束后都会调用 `scripts/preview.sh stop`。
- 项目扫描使用真实 `projects::analyze_project_path` 路径，包含 package manifest、技术栈和 Git 状态探测；Cargo 编译时间不计入扫描数据。
- 日志测试使用生产 reducer 的 300 行单 run 上限；时间线测试使用生产 `buildTaskTimelineRows` 和 `TaskTimeline`。
- 数值用于同一机器/工具链下的回归比较，不等同于所有设备的性能承诺。

## 后续规则

- 只有稳定复现的超预算项进入优化队列。
- 修改 reducer、项目扫描、时间线或启动脚本后，重新运行 `pnpm bench:local` 并更新本页数据。
- 若调整预算，必须在计划或 Review 中记录理由，不能只为让门禁通过而放宽。
