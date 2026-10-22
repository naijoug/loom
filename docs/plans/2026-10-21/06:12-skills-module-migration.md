# Skills Manager 迁入 Loom 实施计划

- 状态：实施中；M0 基础、M1a 来源切片、M1b API 来源与 SSH 配置保护、M2a 安装、M2b 指令及源 8 项测试等价对照、M2c CLI/TUI/ng、M5 内容独立出口已分项通过。G1 仍待完整视觉矩阵、无源码访问与外网 GitHub/GitLab 验收；M3/M4/M6/M7 未完成，G1/G2 均未整体验收。
- 编写时间：2026-10-21 06:12，America/Los_Angeles。
- v2 修订时间：2026-10-21 07:19，America/Los_Angeles；依据本对话的实施就绪审查细化。
- v3 修订时间：2026-10-21 07:29，America/Los_Angeles；补充共享注入块使用者归属、跨客户端未完成事务发现和应用更新准入。
- v4 修订时间：2026-10-21 22:44，America/Los_Angeles；依据 `0b63aad` 的进度审查：解除 M5 出口与 M7 的循环依赖，明确 M2b 等价对照要求，§4 改为实际模块表，补 SRC-09 用户 SSH 配置要求并回填 §9.2。
- 三项补充复核：2026-10-22 00:56，America/Los_Angeles；共享注入块归属、跨客户端事务发现、应用更新准入已对应到 §5.5–5.7、实施任务与 V-16–18；统一入口见 §9.1。本次仅更新计划与索引，不改变阶段验收状态。
- v5 修订日期：2026-10-22，America/Los_Angeles；按实际 API/SSH 验收与源测试逐项对照关闭 M1b、M2b 等价记录及 M5 内容出口，保留 v4 的依赖修正和 G1/G2 边界。证据见 [本轮验收](../../dogfood/2026-10-22-skills-migration-review.md)。
- v6 修订日期：2026-10-22，America/Los_Angeles；M3-01 翻译配置完成自动化与原生保存/重启/清除验收，实际翻译任务和逐 provider 验收仍未完成。
- 用户目标：把 skills 项目的 Skills Manager 功能迁入 Loom，作为 Skills 模块；从 skills 仓库移除管理应用代码与功能。
- 当前交付与验证见 [实施证据](../../dogfood/2026-10-21-skills-migration.md)；两仓均已修改，源项目已有阶段提交。未接管真实用户安装或删除旧应用。
- Loom 基线：`codex/simple-cli-chat` @ `c8313dd60aa50831bce3d7a66ae3fbc20e72f273`，工作区另有未提交的 Chat Markdown、消息分隔、权限文档等改动。
- skills 基线：`main` @ `a245966b15f75faf929b3cdfc6db7909d010e08c`，另有未跟踪 `.agents/`、`skills-lock.json`，不作为待删除应用资源。
- 源仓库：`/Users/guojian/Workspace/naijoug/skills`；目标仓库：`/Users/guojian/Workspace/naijoug/loom`。这些绝对路径仅用于本次迁移定位，不进入产品运行配置或构建依赖。
- 执行依据：[需求](../../requirements.md)、[架构](../../architecture.md)、[开发指南](../../development.md)、[验证矩阵](../../testing.md#开发变更验证矩阵)、[安全策略](../../security-policy.md)。

## 1. 目标与范围

### 1.1 最终形态

1. Loom 在 Settings 侧边栏提供 Skills 板块，进入后完成技能发现、来源管理、详情和安装到各工具等操作；不新增 Chat 同级 Skills 入口。该位置按用户实施中补充要求更新，主题、语言与项目上下文共用 Loom 公共层。
2. skills 仓库只维护技能内容及其质量工具，能够在没有 Loom 源码、Node workspace 或 Tauri 环境时完成内容检查。
3. Loom 把 skills 仓库视为普通的可配置来源，同时支持其他本地目录和远程仓库；运行与构建不依赖相邻源码目录。
4. 文件、进程、网络请求、安装清单和持久化归 Rust 服务；React 仅负责交互、展示和有类型的 IPC。
5. 迁移完成后只有 Loom 仓库维护管理功能；旧 Manager 的应用、启动和发布入口退出 skills 仓库。

### 1.2 计划采用的默认决策

- 保留现有用户可用的管理能力，包括 CLI/TUI、`ng`、自动注入、slash wrapper 和四个已注册翻译 provider；不因桌面迁移隐式删减能力。
- CLI/TUI 迁入 Loom 作为兼容工具；桌面安装、托管指令与恢复、CLI/TUI 收敛分别在 M2a/M2b/M2c 交付。允许旧工具在迁移期保留用于对照，最终与桌面调用同一 Rust 服务。
- 独立 Web 产品、本地 HTTP API 和旧桌面壳在功能替代验收后退役。Loom Web 预览继续服务 UI 验证，不获得真实文件写入权限。
- 默认以本地目录来源接入个人 skills 仓库，不将整个个人技能集合随 Loom 打包。首次使用可以为空库，用户主动添加来源。
- 保持 Loom 当前 React 主版本，适配迁入组件；不把 React 升级作为迁移前置任务。
- M0 已把 Skills 写入现行需求；实施中用户进一步明确入口为 Settings → Skills。保留 Chat/Settings 两个顶层视图，原简化 Chat 约束的其余部分仍适用。
- 采用两个交付门槛：G1 为 Loom Skills 核心可用，G2 为完整替代并允许删除旧应用。翻译账户或 CLI/TUI 分发阻塞 G2，不阻塞已验证核心能力的 G1。
- 远程技能按不可变 revision 快照安装；刷新来源只发现新版本，更新安装需独立操作。本地 symlink 保留实时开发语义，并在 UI/CLI 明确标记。

### 1.3 非目标

- 不恢复 Task/Board/Planning、冻结 Workbench、SDK worker 或根 workspace 的新内核。
- 不构建技能市场、云同步、团队权限、独立公共 SDK 或新的插件执行运行时。
- 不自动加载所有技能到聊天提示词，不把安装状态等同于 CLI 已加载或实际调用。
- 不新增定时任务调度器；cron 类技能仍由外部调度工具触发。
- 不删除技能正文、`archive/skills/`、私人缓存、凭据、已安装技能或旧 Task 数据。
- 不迁移旧应用签名证书、公证配置和品牌资源来替代 Loom 的发布身份。

## 2. 已核实的现状与证据边界

以下来自编写计划时的源码与文档阅读，不代表重新运行后的验证结果。

| 范围 | 当前事实 | 迁移影响 |
|---|---|---|
| Loom | `src/App.tsx` 当前挂载 Chat / Settings；需求当前明确暂不扩展 Skills | 新页面、状态与需求需一起调整，切页保持 Chat 挂载 |
| Manager | `apps/skills-manager-web`、`skills-manager-api`、`skills-manager-desktop`、`skills-manager-tui` 及五个共享包 | 按职责拆分，不整体搬运 monorepo |
| UI | `apps/packages/skills-platform/src/SkillsAdapter.ts` 已划分浏览、来源、翻译和安装接口；UI 使用 React 19，Loom 使用 React 18 | 复用交互规则与测试，适配 Loom 生命周期及依赖 |
| Rust | Manager 桌面 `src-tauri/src/lib.rs` 约 3,400 行，包含扫描、安装、来源、翻译和运行时路径 | 拆成服务模块，剥离旧应用初始化和全局路径单例 |
| 安装 | 桌面和 CLI 共享 `.skills-linker-manifest.json` 约定，另有旧 TSV 兼容 | 保留清单语义、托管边界和旧格式读取 |
| 功能差异 | CLI 支持 `inject.md` 托管注入、doctor、自定义目标；桌面目前不执行注入同步 | 以两端能力并集列清单，不只复制桌面 UI |
| 共享指令 | Codex 与 Amp 的项目级安装目录不同，但注入均写项目根目录 `AGENTS.md` | 锁必须覆盖共享指令文件，不能只按技能目录互斥 |
| 来源兼容 | 桌面支持 GitHub/GitLab 的 HTTPS、SCP 风格 SSH 和 `ssh://`；GitLab 支持多层 namespace | 纳入 URL、旧 ID、认证和刷新兼容矩阵 |
| 刷新语义 | 旧桌面对 cache 原地 `git pull --ff-only`，symlink 安装直接指向来源目录 | 迁移后把来源刷新和已安装版本更新分开 |
| 数据 | 默认位于 `~/Library/Application Support/com.naijoug.skills-manager/`；可有显式数据目录和旧 `.skills-manager-data` | 支持可预览、可重试、可恢复的数据导入 |
| 打包 | 旧桌面 `build.rs` 从仓库 `skills/` 复制随包资源 | 移除该耦合；验证无源仓库时 Loom 仍可运行 |
| 质量工具 | `apps/scripts/skills-quality-check` 调用 TUI 目录中的 routing 工具 | 质量工具与管理代码必须分开迁移 |

源证据入口（路径均相对于 skills 仓库）：`apps/docs/skills-manager.md`、`apps/packages/skills-platform/src/SkillsAdapter.ts`、`apps/skills-manager-desktop/src-tauri/src/lib.rs`、`apps/skills-manager-desktop/src-tauri/build.rs`、`apps/skills-manager-tui/skills-linker`、`apps/scripts/tests/`。

源运行说明中 2026-09-30 的测试、签名候选包等记录仅作历史参考；不作为本次迁移、当前技能数量、真实翻译或 Loom 发布的通过证据。

## 3. 功能与文件去向

### 3.1 功能对照

| 功能 | 最终归属 | 完成要求 |
|---|---|---|
| 本地发现、分类、搜索、详情、相关文件预览 | Loom Skills | 兼容现有 frontmatter、`skill.yaml`、根目录和多层技能；支持仅有 `SKILL.md` 的外部来源 |
| 本地来源、GitHub/GitLab clone、刷新、移除 | Loom Rust | HTTPS/SSH、多层 namespace、旧 ID 兼容；来源与项目安装目标独立；远程刷新不改变已安装快照；移除本地来源只移除登记 |
| 原 Web GitHub API 读取来源 | Loom Rust | 旧来源可读取、刷新、迁移；默认分支、truncated tree 和错误语义保留；读取型来源安装前需物化并校验完整包 |
| copy / symlink、冲突策略、状态、卸载 | Loom Rust | Codex / ChatGPT alias / Claude Code / Amp，全局与项目；避免 alias 导致同一路径重复写入 |
| 自定义目标、doctor、JSON 输出 | Loom CLI + 共用服务 | 保留原 CLI 能力；显式目标授权，不把任意前端路径直接用于写入 |
| auto/global 注入、manual slash wrapper | Loom Rust | 默认保留正文及 marker 约定；manual 显式调用策略不变；cron 不隐式注入 |
| `skills-linker`、fzf TUI、`ng` | Loom `tools/skills-cli/` | 兼容参数、默认行为、退出码及核心 JSON 字段；来源改为显式参数或已配置来源 |
| 翻译 summary / markdown | Loom Skills + Rust | 保留 OpenAI、OpenRouter、Local Codex、Local Claude Code；本地翻译进程有超时、取消、脱敏日志 |
| 复制、导出分享文本、命令面板 | Loom Skills | 保留现有本地操作；复制分享文本不变成自动创建 Gist 或外发内容 |
| 外观、语言、导航、About | Loom 原有公共层 | Skills 是设置侧边栏的板块，复用 Settings 壳；不嵌套旧 Manager Settings，不引入第二套主题或顶层应用入口 |
| 元数据、归档、routing 检查 | skills `scripts/quality/`、`tests/quality/` | 在 skills 仓库独立运行，`perfect` 仅表示评分管道验证 |
| Web/API 服务、旧桌面入口与专属发布脚本 | skills 中退役 | 替代能力及剩余引用检查通过后删除 |

Agent 安装能力与 Chat 运行能力分别声明。保留 Amp 安装不代表新增 Amp Chat；Loom 探测到 OpenCode/Grok 也不代表已实现它们的 Skills 安装。未验证的能力显示不支持或未验证。

G1 包含本地来源与 Git clone、浏览/详情、copy/symlink 安装与卸载、显式更新、冲突处理及 Chat 隔离。G1 可用版本只展示已验证动作；auto 注入、CLI/TUI、翻译和旧数据导入未完成时明确显示阶段边界，不冒充完整替代。GitHub API 来源移至 M1b，不作为 G1 前置条件。

### 3.2 来源兼容矩阵

| 编号 | 输入或历史数据 | 目标行为 | 验收方式 |
|---|---|---|---|
| SRC-01 | 本地技能目录、根目录 `SKILL.md`、多层技能目录、含空格/中文路径 | 只登记与扫描；根目录和选中项目分离；排除 `.git`、依赖/缓存及明确归档目录 | 本地 fixture、重启、移除登记后原目录仍存在 |
| SRC-02 | `https://github.com/owner/repo[.git]` | 同仓库标准化，不因 `.git` 或末尾 `/` 重复导入；旧 HTTP 输入规范化为 HTTPS | URL 用例及本地 Git fixture |
| SRC-03 | `git@github.com:owner/repo.git`、`ssh://git@github.com/owner/repo.git` | 逻辑来源与 HTTPS 同仓库关联，保留用户选择的 transport | 标准化与进程参数测试、SSH 路径真实验收 |
| SRC-04 | GitLab HTTPS、`group/subgroup/repo`、`/-/tree/main` 页面链接 | 保留完整 namespace，页面路径不混入仓库名；旧 HTTP 输入规范化为 HTTPS | 复用源 GitLab 测试并扩充多层子组 |
| SRC-05 | GitLab SCP 风格 SSH、`ssh://` | 完整 namespace 与 transport 保留；不误按 GitHub owner/repo 截断 | URL/参数 fixture、代表性真实 GitLab 仓库 |
| SRC-06 | 旧 `github:*` / `gitlab:*` group ID 与 `encodeURIComponent(groupId)::encodeURIComponent(relativePath)` skill ID | 保存旧 ID alias 到新 source/skill identity；迁移后仍能识别原安装，重名不自动合并 | 旧 JSON/路径编码/同名跨来源 fixture |
| SRC-07 | Web `github-api`、`server-cache`、桌面本地 cache 记录 | 保留 provider、模式、默认分支和来源身份；同仓库多模式只生成明确的合并建议 | M1b/M4 旧库对照，不静默丢记录 |
| SRC-08 | GitHub API 默认分支变更、truncated tree、分页、二进制相关文件 | 固定到同一 commit 读取树及文件；不完整包拒绝发布快照/安装，保留上一成功版本 | 合成 API 响应和代表性真实读取 |
| SRC-09 | 私有仓库、SSH key/agent 不可用、未知 host key、超时、限流 | 使用已有凭据通路，失败可诊断；不弹出无限等待、不关闭 SSH host 校验、不记录凭据；保留用户已有的 `GIT_SSH_COMMAND`/`core.sshCommand`（自定义 key、ProxyJump 等），仅在用户未配置时补 `BatchMode=yes` 防交互。M1b 已修复 `0b63aad` 的无条件覆盖：环境通路优先，有效 core.sshCommand 键名探测失败即拒绝，不回退覆盖用户配置 | 错误 fixture；自定义 sshCommand fixture 验证不被覆盖；具备权限时真实验证 |
| SRC-10 | 非支持 host、自建 GitLab、任意 shell 片段 | 不把泛称 Git 支持解释为新增任意远程协议支持；未支持输入清晰失败，无 shell 拼接 | 输入校验与注入测试 |

身份标准化兼容旧大小写处理，但保存原始 namespace；遇到归一化碰撞显式冲突，不合并两个可区分来源。URL 中的 token/userinfo 不持久化为普通来源字段，凭据由后端单独处理。

### 3.3 源码处理清单

- `apps/packages/skills-ui/`：抽取列表、详情、来源、安装和翻译组件及相关测试，迁入 Loom `src/features/skills/`；替换应用级状态、全局 CSS、默认 mock adapter 和私有主题逻辑。
- `skills-core`：保留模型与解析用例；文件扫描、解析与稳定 ID 的权威实现收敛至 Rust。前端只保留展示逻辑和契约类型，不长期维护两套解析器。
- `skills-platform`：把桌面调用迁入 Loom 的类型化 Skills API；旧 Web adapter 随独立 API 退役，mock 仅留测试/预览。
- `skills-installers`：迁移测试语义到 Rust 安装服务；Node 文件系统安装实现不进入浏览器包。
- `skills-translation`、API 中特有能力：按实际注册能力迁入 Rust；未注册 Amp 翻译实现不自动升级为新产品功能。
- `skills-manager-desktop`：迁出业务与测试，退役旧 Tauri 壳、标识、打包资源与专属依赖。
- `skills-manager-tui/{skills-linker,tui,ng,managed_instructions.py}`：管理能力迁入 Loom。Loom 内部在 M2c 已收敛到桌面/CLI 共用的 Rust 服务；源仓库旧实现仅作对照并保持 v2 写保护，G2 后在 M7 随旧应用一并删除；Loom 不留跨仓库运行依赖。
- `trigger_examples_tool.py`、`run_trigger_eval.sh`、`trigger_eval_report.py`、`predictor_adapter_template.py`、`check-skill-metadata.py`：移入 skills 的质量工具目录。
- `test_managed_instructions.py` 的管理行为用例在 Loom 建立逐项等价测试（M2b 出口），源文件随旧应用在 M7 删除；`test_skill_metadata.py`、`test_skill_lifecycle.py`、`test_trigger_tools.py`、`test_who_am_i.py` 留在内容仓库并更新 `support.py` 和引用。
- `apps/scripts/skills-quality-check`：改为 skills 根目录的 `scripts/skills-quality-check`；不再要求应用 workspace 或 Loom 可用。
- `scripts/skills-manager-*`、`start-local.sh`、`preview.sh`、`debug.sh`、`package-macos.sh`：逐个核对调用方，仅删除 Manager 专属入口。
- `apps/package.json`、workspace、lockfile、tsconfig 及包依赖：消费者迁出后移除；技能自身资源目录里的 `package.json` 不在删除范围。
- Manager 文档与历史计划：有效操作说明迁入 Loom；源仓库保留简短迁移说明，历史资料标记退役并修复有效链接，不伪造仍可执行的命令。

## 4. 目标架构与契约

下表按 `0b63aad` 及 M1b 切片的实际模块记录；标“待新增”的仍为拟定路径。数据模型分散在各模块，未设集中 `models.rs`；没有独立 `service.rs`，桌面 `commands.rs` 与 CLI `cli.rs` 共同调用 repository/installers/instructions/operations，只有 `commands.rs` 依赖 Tauri。

```text
Loom
  src/features/skills/              SkillsPanel、InstallPanel、InstalledSkills、展示状态与请求归属
  src/features/settings/           设置侧边栏中的 Skills 入口与公共壳
  src/api/skills.ts                 复用现有 tauriClient 的类型化调用
  src-tauri/src/skills/
    error.rs                       有类型错误码
    files.rs                       私有、有界、原子元数据读写
    coordination.rs                用户级生命周期 lease、writer 锁、PendingTransaction 索引
    runtime_paths.rs               桌面/CLI 共用库目录与固定协调目录解析
    repository.rs                  来源库持久化与来源目录解析
    sources.rs                     GitHub/GitLab URL、transport、身份与旧来源只读映射
    github_api.rs                  固定主机 API、commit/tree/blob 完整包读取与校验
    snapshots.rs                   revision 快照发布与校验
    operations.rs                  长操作记录、取消、Git 进程执行与 HTTP 脱敏日志
    catalog.rs                     扫描、解析、检索、详情
    targets.rs                     工具/作用域目标、指令与 wrapper 路径授权
    package.rs                     技能包状态、指纹、复制与链接
    transactions.rs                prepare journal、提交、回执与恢复
    installers.rs                  安装/更新/卸载规划与执行、清单 v2、恢复入口
    instructions.rs                托管注入、wrapper、共享使用者 ledger、诊断
    cli.rs、cli/interactive.rs     无窗口 CLI 与 tui/ng 交互
    commands.rs                    Tauri 输入校验与薄封装
    translation.rs、translation/config.rs  M3-01 provider 私有配置；翻译执行待 M3-02
    migration.rs                   待新增（M4）：旧数据规划、导入、恢复
  src-tauri/src/bin/loom-skills.rs  无窗口 CLI 入口
  tools/skills-cli/                 skills-linker / tui / ng 启动器

skills
  skills/                          技能包与技能自身运行资源
  archive/skills/                  原有归档
  scripts/quality/                 内容检查和 routing 工具
  scripts/skills-quality-check     内容检查唯一入口
  tests/quality/                   内容测试
  docs/                            内容编写、迁移和恢复说明
```

- 不放进冻结的 `packages/`、`crates/` 或 `workbench/`。先在当前桌面 Rust crate 内分层，不新增公共 SDK。
- CLI 服务接收显式路径和上下文，不初始化 Tauri 窗口；新增 binary 后固定默认桌面运行目标，检查 Tauri 打包不因多 binary 歧义失败。
- IPC 使用 `skills_` 前缀，覆盖来源、目录、安装、doctor、翻译、迁移规划/执行/状态。M0 固化完整命令、错误与事件字段，并加入 `contracts/tauri-contract.json` 和两端测试。
- 长操作返回 `operationId`，事件携带来源/目标/项目标识、阶段、结果和可重试错误；取消、退出、失败有明确终态。不能在同步 command 中无界 clone 或等待本地 Agent。
- 复用 Loom 进程监督与脱敏日志能力，但 Skills 操作不创建 ChatSession、不推进回合、不持有 Chat 的全局运行锁。
- 前端异步结果绑定 source/skill/project/request，切换页面或项目后不能写回错误目标。写操作捕获发起时的目标，不跟随当前选中的项目变化。
- 新来源使用稳定 `sourceId`，技能以来源和相对路径定位；显示 ID、安装名称和安装目标分离。旧 ID 建映射，路径迁移不能把同一安装误判为新技能。
- Chat 正在运行时切换 Skills 不取消回合、不清草稿。安装完成只报告文件状态；CLI 是否需要新会话/重启由适配器和实测说明，不悄悄清除续聊句柄。

### 4.1 数据模型最小字段

以下为实施契约草案；M0 固化字段名、JSON 样本及 schema 升级规则，不要求一次实现所有 provider。

| 对象 | 必需内容 | 约束 |
|---|---|---|
| Source | `sourceId`、kind/provider、规范化位置、transport、`legacyIds`、`latestRevision`、最后成功刷新时间 | 本地来源 revision 可空；URL/位置不含 secret |
| Snapshot | `sourceId`、`revision`、`contentDigest`、受管路径、完成标记 | 完成前不可被目录或安装引用；远程快照发布后只读使用 |
| Skill | `sourceId`、包相对路径、显示 ID、`installName`、category、metadata | 文件访问由后端通过 source + relativePath 解析，不接受 UI 提供任意绝对路径 |
| Installation | `installationId`、目标实际路径、来源/包身份、mode、revision 或 `local-live`、安装内容/链接指纹、managed 文件清单 | 脱离来源目录也能查询和卸载；同名不同来源必须冲突或显式替换 |
| SharedInstruction | canonical 指令文件路径、marker、内容指纹、使用者集合（installationId 与安装清单位置） | 跨 Agent/dataDir 共用归属记录；最后一个已核实使用者移除后才能删块；不同内容不能覆盖共享块 |
| Operation | `operationId`、幂等请求 ID、动作、捕获的目标、阶段、前后指纹、备份/恢复位置、逐项结果 | journal 不含完整技能正文或凭据；重试依据状态而非重复执行命令 |
| PendingTransaction | `operationId`、schemaVersion、canonical write set、所属 dataDir、恢复日志位置与校验信息、提交状态 | 位于固定用户协调目录，跨客户端可发现；修改目标前持久化，完成核对后才清除；不包含凭据或正文 |
| Migration | `migrationId`、源数据指纹、规划版本、ID 映射、逐项检查点、冲突和恢复状态 | 来源变化或目标变化使旧规划失效，需重新规划 |

目标目录下的安装清单是安装归属的权威记录；应用库保存查询索引，不依赖当前来源可读性判断是否托管。继续读取旧 JSON v1/TSV；旧记录缺少 digest 时标 `legacy-unverified`，核对真实目标后才能接管，不凭同名推定归属。M0 确定扩展字段和写入版本；需要不兼容升级时保留原文件与恢复路径，不能让旧工具继续写新清单。

### 4.2 IPC 与操作生命周期

| 用例 | 拟定命令族 | 关键返回/前置条件 |
|---|---|---|
| 库、详情、安装目标/状态 | `skills_list_library`、`skills_get_detail`、`skills_list_targets`、`skills_get_installations` | 稳定身份、当前 revision、能力与错误；只读无注入副作用 |
| 来源添加/刷新/移除 | `skills_add_source`、`skills_refresh_source`、`skills_remove_source` | 长任务 operationId；刷新返回可用新版本，不更新安装 |
| 安装/更新/卸载 | `skills_plan_install`、`skills_apply_install`、`skills_update_installation`、`skills_uninstall` | planId/预期指纹；捕获目标；changed target 返回冲突而非继续覆盖 |
| 指令检查、翻译 | `skills_doctor`、`skills_list_translation_providers`、`skills_save_translation_config`、`skills_translate` | doctor 只读；凭据返回仅 configured 状态；翻译有取消入口 |
| 迁移 | `skills_plan_migration`、`skills_apply_migration`、`skills_rollback_migration` | 规划与执行分离、逐项结果、原数据保留 |
| 进度与恢复 | `skills_get_operation`、`skills_list_operations`、`skills_cancel_operation`；`loom://skills-operation` | 订阅后补查快照，事件有递增序号；切页/重启不丢最终结果 |

操作阶段：`planned → preparing → waiting_for_lock → applying → committed`；失败进入 `failed` 或 `recovery_required`，取消在安全检查点进入 `cancelled`。提交中的取消先完成当前原子文件动作，再补偿或保存恢复状态，不在删除旧文件后直接退出。事件只是展示投影，持久化 operation 记录决定最终状态。

错误至少区分 `invalid_input`、`unsupported_source`、`conflict`、`source_unavailable`、`auth_required`、`lock_busy`、`permission_denied`、`cancelled`、`recovery_required`；前端不靠解析自然语言判断可重试性。为进程监督新增明确 Skills operation 归属，并接入正常退出、强制停止及启动恢复；不能仅注册子进程而遗漏文件事务收口。

纯文件安装、卸载、迁移及回滚也必须持有 `supervisor().begin_operation()` 返回的 operation guard，直到提交或安全恢复状态落盘；应用“安装更新并重启”通过 `begin_update()` 判断准入。跨进程 CLI 另用第 5.7 节的生命周期准入锁协调，不能依赖桌面进程内的计数。

## 5. 持久化、安全与数据迁移

### 5.1 新存储与边界

- 使用 Loom 应用数据目录的 `skills/` 子目录管理来源库、配置、缓存和完整操作记录；固定用户协调目录保存锁、共享注入块归属及未完成事务索引。两者带 schemaVersion，自定义 dataDir 不隔离协调状态。已有 `.loom/chat/v2` 不变。
- 项目目标基于后端校验的已登记 canonical project；全局目标由后端解析当前用户目录。CLI 自定义目标需显式指定并校验；拒绝任意删除根目录、路径穿越和 symlink 逃逸。
- 安装校验完整技能包，保留 `agents/`、脚本和 references；只读取和复制内容，不执行来源中的代码。详情不渲染原始 HTML，不自动加载外部资源。
- 托管安装必须同时核对清单、目标与实际文件/链接；未托管内容不能因同名被卸载。批量操作按项返回结果，部分成功不显示为整体成功。
- 桌面/CLI 的文件提交使用同一用户级跨进程 writer 锁，覆盖安装目录、清单、共享 `AGENTS.md`/`CLAUDE.md`、wrapper、快照引用和迁移恢复；网络、翻译和只读扫描不持锁。具体协议见第 5.3 节。
- 删除缓存来源前检查安装引用：被 symlink、未完成 operation 或回滚记录引用的快照不能删除。首版不做自动 GC；移除来源可只移除库登记并保留被引用快照，明确展示占用原因；需要清理时先卸载/转换引用。
- 翻译配置凭据只由后端读写，Unix 文件权限 `0600`；不返回前端、不进入诊断包和日志。迁移报告只列配置存在性，不列 secret 值。
- 自动注入仅修改自身 marker 管理的块，并按第 5.5 节核对使用者集合；异常/重复 marker 返回错误，不重写用户其他指令。禁止用 prompt 代替文件写入校验。

### 5.2 迁移流程

1. **发现与预览**：检测旧默认 app data；额外数据目录由用户指定，不扫描整个 HOME。列出来源、安装清单、缓存、失效链接、配置和冲突，生成无 secret 的迁移规划。
2. **建立快照**：执行前停止旧 Manager 写入；备份将修改的清单、链接、wrapper、注入文件及配置，记录版本、哈希与 `migrationId`。仅列用户允许操作的目标。
3. **暂存与校验**：复制旧库和必要缓存到 Loom staging；保留 source/skill 映射、默认分支和安装名称。来源目录与安装项目分别映射，不能把旧 workspace 自动当成所有项目目标。
4. **安装兼容**：继续读取 JSON 与 legacy TSV 清单；已有 copy 内容不强制覆盖。指向旧 cache 或旧 `.app` bundle 的 symlink 需要转接到稳定来源或显式转为 copy；无法恢复的包单独报错，不静默重新下载不同版本。
5. **提交与恢复记录**：先校验暂存数据，再原子切换新库；外部目标逐项操作并记录补偿信息，不宣称跨文件系统全局原子。失败支持从日志重试或恢复，不重复安装、不覆盖较新用户改动。
6. **重启验证**：重新启动后验证来源、详情、provider 配置存在性、安装状态和链接解析；相同迁移重复执行必须幂等。
7. **切换完成**：保留旧数据和备份供恢复。完成代码退役不等于授权清理旧 app data、已安装应用或私人缓存，这些不在本迁移默认删除范围。

回滚时先停止 Loom Skills 写入；只恢复仍匹配本次操作后哈希/链接目标的文件，后续用户改动转为冲突报告。旧源码可通过记录的 Git 基线恢复；使用旧应用前恢复与其兼容的清单/路径。迁移期间禁止两个管理器同时写同一目标。旧 Manager 的 Node/Rust/CLI 写入口增加清单版本预检，在目标变更前拒绝 v2/损坏清单；这只保护更新后的旧源码，已安装历史二进制仍必须停止使用。

### 5.3 共享文件锁与写入协议

首版采用一个用户级 writer 锁，以减少多资源锁排序和死锁实现成本；性能确有证据不足时再细化。它串行化短时文件提交，不串行化 Chat、网络拉取或模型请求。

1. 桌面与 CLI 使用同一锁根：当前操作系统用户的固定 Loom 协调目录；自定义 library/dataDir 不能改变锁域。测试通过显式依赖注入提供统一锁根，不接触真实用户目录。
2. 使用 OS 文件锁，持有期间不删除/重建锁文件，进程退出后由系统释放。拿不到锁返回 `lock_busy` 或进入可取消的有界等待。
3. 从已校验的目标推导完整 write set：技能目录、manifest、所有 wrapper、注入文件及其使用者记录、安装索引、快照引用和 operation journal。Codex 与 Amp 项目目标虽然不同，但共享 `project/AGENTS.md`，都受同一锁保护。
4. 获取锁后先检查固定协调目录的未完成事务索引；与 write set 重叠的记录必须按第 5.6 节恢复或拒绝操作。之后重新读取实际目标和文件指纹，核对规划时的版本；canonical 路径与路径组件变化必须重新校验。预先读到的数据不能直接用于覆盖。
5. 在目标文件所在文件系统创建 staging/backup，先持久化 prepare 日志，再持久化共用 PendingTransaction，之后才替换任何目标。指令与 wrapper 不丢用户正文和权限；部分提交故障保留逐文件前后状态，进入恢复流程。
6. 清单、使用者集合、引用与文件结果一致后记录 committed，核对并清除对应 PendingTransaction，再释放锁；回滚和重试使用同一协议。扫描也检查待恢复索引，只能展示旧完成状态、新完成状态或“操作中/待恢复”，不能将半成品判为已安装。
7. 用户编辑器和旧 Manager 不参与锁协议：提交前指纹复核发现变化则冲突退出；不承诺与任意外部 writer 全局串行。切换期间停止旧 Manager，界面说明同时手工编辑受管文件可能产生冲突。

锁验收必须包含：两个共用服务的独立测试进程分别向 Codex 与 Amp 安装 auto skills，最终 `AGENTS.md` 同时包含两个完整托管块且用户正文不变；反向顺序安装/卸载无死锁；杀死持锁进程后可取得锁并识别待恢复操作；不同 dataDir 的客户端仍不能绕过同一用户锁。M2b 用测试进程完成服务级验证，M2c 再从真实桌面与 CLI 入口复验，避免把尚未实现的 CLI 作为 M2b 前置条件。

### 5.4 来源刷新、快照与安装更新

1. **远程刷新**：在独立 staging fetch/读取固定 commit，校验完整技能包和 digest，发布到受管 `sources/<sourceId>/snapshots/<revision>/` 后，才更新目录的 latestRevision。不得在已被安装引用的目录原地 pull。
2. **安装固定版本**：远程 copy 和 symlink 均记录 revision；symlink 指向该 revision 的真实快照路径，不能指向可变 `current` 链接。快照损坏时拒绝继续使用/安装，报告修复需求。
3. **显式更新**：刷新仅显示“有新版本”。用户选择更新某项安装后生成写入计划；用户改过 copy 时先报冲突，确认替换才更新文件、wrapper/注入、清单和引用。Chat 运行中的技能不会因单纯刷新而被改变。
4. **本地开发来源**：local-live symlink 的内容随用户编辑即时变化；UI/CLI 明确标记，revision 为空。local copy 安装记录内容指纹，来源变化只显示可更新。Loom 不在用户本地源目录执行 pull 或清理。
5. **旧链接迁移**：旧 cache 若有未提交修改，保存实际内容并用内容 digest 标识迁移快照，不冒充纯 commit；旧 bundle 内容同样先物化稳定快照。迁移成功前不删除旧源，不能重新下载最新版冒充原内容。
6. **更新恢复**：旧 revision 在安装、进行中操作及回滚记录引用解除前保留。操作失败恢复原链接/内容和清单；无法自动恢复时隔离该安装并给出 recovery_required，不能显示成功。

专项验收：安装 revision A → 远程更新到 B → 点击刷新，安装内容仍为 A → 显式更新后为 B → 故障回滚后为 A；B 删除/改名某技能时 A 的链接仍有效；刷新失败不改变 latestRevision；本地 live 行为与远程固定行为分别测试。

### 5.5 共享注入块的使用者归属

1. 以 canonical 指令文件路径与 marker 为共享资源键，在固定用户协调目录保存 SharedInstruction。使用者是可定位到目标安装清单的 installationId 集合，不使用容易重复增减的裸计数；不同 dataDir 看到同一份归属记录。
2. 同一技能安装给 Codex/Amp 且需要相同注入内容时，复用一个块并添加使用者；重复安装/恢复不得重复添加。安装清单中的 managed 文件引用、共享集合和正文纳入同一事务。
3. 卸载只移除本次 installationId。其他已核实使用者仍存在时保留块；最后一个使用者移除且块指纹仍匹配时才删除。用户修改过的块返回冲突，不按 marker 直接删除。
4. 同一 marker 对应不同内容时返回 `conflict`；不同 revision 即使注入文本相同也要保留各使用者的版本关联。更新其中一个安装若会改变其他使用者仍依赖的内容，拒绝单独覆盖，要求重新规划全部受影响安装的一致更新。规划检测共享路径并标记 coordinated，选齐使用者后将技能、各安装清单、去重的指令和归属纳入同一个 prepare journal；部分失败整组补偿或保留待恢复状态，不把普通逐目标提交冒充一致更新。
5. 旧 marker 缺少使用者记录、清单不可读或外部删除时标为归属未核实；M4 结合已授权目标清单重建，不能把无法读取解释为零使用者并删块。遇到无法确认的旧使用者保留原块并报告冲突。

专项验收：同一 auto skill 安装给 Codex 与 Amp → 卸载 Codex → Amp 所需块仍在 → 卸载 Amp → 块才删除；覆盖重复安装、反向卸载、同 marker 不同内容、更新冲突、失败恢复及迁移旧块。与 V-02 的“两个不同技能并发写入”分别验证。

### 5.6 跨客户端发现与恢复未完成事务

1. PendingTransaction 与 writer 锁同属固定用户协调目录，不放在可切换的 dataDir 内；目录仅当前用户可访问，记录只含恢复定位信息。所有写客户端和安装状态查询都使用它。
2. 先使完整 prepare 日志、备份及其引用持久化，再原子发布并同步 PendingTransaction，最后修改受管目标。M0 探针验证实际平台的文件与目录同步顺序；不将“rename 成功”直接等同于崩溃后可恢复。
3. 每次获得 writer 锁后扫描相关索引。A 崩溃后，B 即使使用不同 dataDir，也必须发现 A 对相同目录、manifest 或共享指令的待完成修改。索引未读到不应由任意解析失败静默回退为空。
4. 恢复器验证记录版本、路径边界和前后指纹后才能续做或补偿；缺日志、备份损坏、权限不足或未知 schema 时返回 `recovery_required`，保留记录并拒绝重叠写入。有效索引中不相交的 write set 可继续；索引整体损坏、无法确定影响范围时停止新的受管写入。
5. 恢复完成并核对文件、清单、共享使用者及引用后才清除标记。若在 committed 日志之后、标记清除之前崩溃，B 核验后只完成收尾，不重复安装或回滚已提交结果。仅进程消失/锁释放不能触发清除。

专项验收：A 在替换文件后、写清单前崩溃 → B 使用另一 dataDir 尝试同目标写入 → 先恢复或被明确阻止；覆盖共享 AGENTS 重叠、无关目标可继续、缺失日志、损坏索引和 committed 后残留标记。该服务级场景在 G1 前通过，M2c 用真实 CLI 再验。

### 5.7 Skills 操作与应用更新的准入

1. 桌面 Skills 写操作在准备阶段进入前取得 operation guard，覆盖等待 writer 锁、文件提交与恢复记录落盘；即使没有子进程也计入 supervisor。不能在 spawn/register 或最后保存前提前释放。
2. 桌面“安装更新并重启”保留 `begin_update()` 的空闲检查：存在 Skills operation 时延后更新，不强行取消事务。更新准入成功后拒绝新 Skills 写操作；更新安装失败释放准入，恢复正常操作入口。
3. 独立 CLI 的运行不在桌面 supervisor 内计数。桌面/CLI 的 Skills 操作另持固定用户协调目录中的共享生命周期 lease；更新安装取得其独占 lease，无法取得则延后。lease 覆盖 CLI 的准备至终态以及依赖随包资源的使用期，读取型 CLI 使用随包程序时也持 lease；网络准备阶段不持 writer 锁。
4. 锁顺序固定为生命周期 lease → 进程内 operation/update guard → writer 锁；禁止持 writer 锁后反向等待生命周期 lease。更新只尝试独占准入，不持锁等待活动任务结束。取得更新准入后检查未完成事务，存在未解决记录时延后更新；不在独占更新期间调用需要重新获取共享 lease 的恢复入口。
5. 更新独占 lease 保持到安装完成/重启退出或失败返回，由 OS 处理异常退出释放。CLI 在更新期间准入失败即退出并提示稍后重新调用，不能原地等待后让旧进程继续写可能已升级的 schema。新进程仍校验 schema，未知版本拒绝写入。

专项验收：纯文件安装、迁移、回滚中点击“安装更新”均被延后；更新已准入时新桌面/CLI 操作被拒绝；模拟更新失败后准入恢复；外部 CLI 活跃时更新延后；任一进程崩溃不留下永久 lease，待恢复事务仍受第 5.6 节保护。测试使用可控更新安装器替身，不需要为了验证准入而实际安装更新。

## 6. 里程碑与实施任务

核心路径：`M0 → M1a → M2a → G1`。完整替代路径：`M1b + M2b + M2c + M3 + M4 + M5 → M6/G2 → M7`，各阶段依赖见下表。G1 通过后可交付核心模块；M7 仍需 G2。这里的分支表示工程依赖，不要求并行 Agent 或同时修改共享文件。

| 阶段 | 必须先完成 | 交付 | 是否阻塞 G1 |
|---|---|---|---|
| M0 | 无 | 基线、能力矩阵、契约、安全探针 | 是 |
| M1a | M0 | 本地/Git 来源、固定快照、只读页面 | 是 |
| M2a | M1a | 桌面安装/更新/卸载的安全闭环 | 是 |
| M1b | M1a | GitHub API 与旧 Web 来源兼容 | 否 |
| M2b | M2a | 共享指令注入、doctor、跨资源恢复 | 否 |
| M2c | M2b | CLI/TUI/ng 兼容及分发 | 否 |
| M3 | M1a；集成验收要求 M2a | 翻译、复制/分享文本、命令面板 | 否 |
| M4 | M1b、M2b、M3 的 schema 稳定 | 旧数据导入、接管与回滚 | 否 |
| M5 | M0；M5-02 出口要求 M2b 等价对照已记录 | 内容质量工具脱离应用 | 否 |
| M6/G2 | M1b、M2b、M2c、M3、M4、M5，G1 已通过 | 完整替代验收、切换 | 否 |
| M7 | G2 | 源应用移除、双仓收尾 | 否 |

每个任务有稳定 ID。按表中的文件范围提交可 Review 补丁，提交说明记录对应任务、验证命令和未解决问题；不需要一个任务机械对应一个 PR。新增路径均为拟定路径。未通过前置出口时，只推进不依赖该结果的任务。

### M0：固化边界、基线与功能对照

| 任务 | 修改/检查范围 | 可 Review 产出与退出检查 |
|---|---|---|
| M0-01 | 两仓状态、入口、调用方、许可证 | 记录 SHA 与现有 diff；按任务归属保留在途补丁，不 reset/clean；建立迁移文件清单 |
| M0-02 | Loom `docs/requirements.md`、`architecture.md`、`security-policy.md` | 明确 Skills、两个 gate、Chat 隔离和产品非目标，需求与计划无冲突 |
| M0-03 | 拟新增 `docs/architecture/skills-contracts.md`、`contracts/tauri-contract.json` | 固化第 4 节 DTO、错误、事件、ID alias、安装清单、SharedInstruction 与 PendingTransaction schema；明确第 5.7 节更新准入和锁顺序；新增契约注册随命令实现落地，不提前让主门禁失配 |
| M0-04 | `src-tauri/tests/fixtures/skills/`、对应前端 fixture | 收集脱敏/合成 v1 JSON、TSV、不同 provider 来源、CLI 默认行为/JSON/退出码；列出刻意收紧的安全兼容差异 |
| M0-05 | 独立 Rust 定向测试/小型探针 | 验证共用 OS writer 锁、生命周期共享/独占 lease、同文件系统原子替换及同步顺序、快照链接、异常退出解锁、A 崩溃后另一 dataDir 的 B 发现待恢复记录；结果固化为后续回归 |
| M0-06 | 源 check 与内容质量入口、Loom 相关基线 | 在隔离数据目录执行；记录已有失败，区分迁移回归与环境限制 |

出口：每个现有功能、测试和入口有明确去向；写入锁、恢复索引、更新准入和 snapshot 协议通过小型探针；共享块使用者归属、源身份、安装归属与 schema 已固化。此时可以开始 M1a，不能因为“计划已写完”跳过该出口。

### M1a：本地/Git 来源与只读 Skills 模块

依赖：M0。

| 任务 | 文件范围 | 可验证产出 |
|---|---|---|
| M1a-01 | `skills/{models,repository,catalog}.rs` | 空库/来源库读写、根/多层扫描、稳定身份和逐项解析错误；脱离原仓库运行 |
| M1a-02 | `skills/{sources,snapshots}.rs`、进程监督 | GitHub/GitLab URL 和 transport；固定 commit staging、取消、超时、刷新失败保留旧版本；SRC-01–06/09–10 的相应 fixture |
| M1a-03 | `skills/{service,commands}.rs`、`lib.rs`、`src/api/skills.ts`、IPC 契约 | 类型化库/详情/来源命令与 operation 查询、事件；未实现能力明确返回 unsupported |
| M1a-04 | `src/features/skills/SkillsPanel.tsx`、`src/features/settings/SettingsPage.tsx`、i18n | 设置侧边栏 Skills 入口与列表/搜索/详情/来源管理；复用设置壳，保持 Chat/Settings 两个顶层视图；英文/中文、主题、Chat 挂载和草稿正常 |
| M1a-05 | 来源登记/快照引用、相关测试 | 删除本地来源不删原目录；有引用快照不得清除；重启后库与 operation 状态正确 |

出口：来源添加 → 扫描 → 详情 → 刷新 → 移除登记 → 重启完成一轮；真实桌面只读使用通过；源包脚本不被执行，未加载任何安装 mock 到生产。

### M2a：桌面安装、显式更新与卸载闭环

依赖：M1a；此阶段完成后验收 G1。

| 任务 | 文件范围 | 可验证产出 |
|---|---|---|
| M2a-01 | `skills/{coordination,targets,package,installers}.rs` | 后端目标枚举/路径校验、alias 去重、完整 write set、用户 writer 锁；写入前检查共用 PendingTransaction，未知 target 失败 |
| M2a-02 | installers/repository/operation journal | 新安装 copy/symlink、冲突 fail/skip/显式替换、目标 skills 目录同级、同文件系统的私有 staging/backup（避免被技能发现），manifest 与文件一致；批量逐项结果 |
| M2a-03 | snapshots/installers | A → 刷新 B 仍使用 A → 显式更新 B；本地 live 清晰标记；删除来源后凭安装记录卸载 |
| M2a-04 | `skills/{transactions,coordination,installers}.rs`、启动恢复 | 在 staging、替换、manifest 写入和索引清除前后故障注入；另一 dataDir 的客户端先恢复或拒绝重叠写入；缺日志/坏索引/未知 schema 不能当成空状态；不留下假 installed |
| M2a-05 | Skills 安装面板与状态请求 | 默认 copy、作用域/路径/版本可见；无隐式勾选新目标；切项目/技能时旧响应与旧选择不污染 |
| M2a-06 | 定向 Rust/前端、`process_supervisor.rs`、`app_update.rs`、桌面和 G1 证据 | Codex/Claude/Amp 目标 fixture、copy/symlink、冲突/卸载/重启、Chat 切页；纯文件 operation guard 与更新延后/拒绝新操作/失败恢复；跨进程 lease 用测试进程验证；至少一个真实 Agent CLI 发现与显式调用 |

阶段限制：尚未完成的 auto 注入、wrapper 和旧安装接管不能被生产 UI/IPC 承诺为可用。M2a 可安装技能文件，但须清楚说明注入未支持；旧清单只读识别并标 legacy-unverified，不重写。G1 面向新目标/新安装，不代替 M4 的旧数据接管验收。

出口：M2a-01–06 的功能和定向验证完成，提交第 7.1 节 G1 综合验收。CLI 重写、四种翻译账户和旧数据全面迁移不作为此出口条件；G1 是这些任务完成后的独立检查，不与 M2a 形成循环依赖。

### M1b：GitHub API 与旧 Web 来源兼容

依赖：M1a，可在 G1 后交付。

| 任务 | 文件范围 | 可验证产出 |
|---|---|---|
| M1b-01 | sources/snapshots/provider HTTP 封装 | 从默认分支解析一次 commit，树/blob 均绑定该 commit；认证/限流/分页/truncated 明确结果 |
| M1b-02 | 完整包校验、安装入口 | references、agents、脚本及二进制资源完整物化；不以缺文件的 Markdown 预览结果直接安装 |
| M1b-03 | 旧来源 fixture/迁移 alias | `github-api`、`server-cache` 和桌面记录映射无丢失；源码中 GitLab clone 用例迁入；SRC-07–08 通过 |
| M1b-04 | operations/SSH 配置兼容与日志 | SRC-09 保留环境及 core.sshCommand；只在未配置时补 BatchMode；配置读取错误拒绝、诊断不泄露 wrapper 输出 |

出口：API 来源浏览/刷新/安装与错误路径通过；真实 GitHub API 路径单独记录，环境阻塞只阻塞相应能力与 G2。

### M2b：共享指令、doctor 与跨文件恢复

依赖：M2a；复用已完成的锁和 journal，不重建第二套安装后端。

| 任务 | 文件范围 | 可验证产出 |
|---|---|---|
| M2b-01 | `skills/instructions.rs`、installers、协调目录 | auto/global 注入和 cron 排除；manual wrapper 与显式调用策略；SharedInstruction 使用者集合与清单共同提交，最后使用者移除才删块；同 marker 内容冲突及坏块拒绝写入 |
| M2b-02 | doctor/安装 UI/API | current/stale/malformed/missing 等诊断；doctor 不写文件；安装/卸载影响清单在操作前可见 |
| M2b-03 | 多进程 fixture、coordination、共享归属 | Codex/Amp 共享 AGENTS 并发、同技能多使用者部分卸载保块、最后使用者删除、重复安装/版本冲突、反向操作、不同 dataDir 同锁、杀进程解锁全部通过 |
| M2b-04 | journal/恢复测试 | 在技能文件、wrapper、注入、使用者记录和 manifest 各阶段失败；共用索引可发现，共享归属与正文一同恢复或明确逐项待恢复，不覆盖后续用户编辑 |

出口：完整指令行为和安全恢复通过，再启用相应 UI/IPC；并在实施证据中记录源 `test_managed_instructions.py` 8 个用例（`test_sync_update_remove_preserve_user_bytes_and_other_blocks`、`test_malformed_block_leaves_file_unchanged`、`test_crlf_and_symlink_are_preserved`、`test_status_does_not_create_files`、`test_unsupported_manifest_blocks_force_install_and_uninstall`、`test_copy_refresh_requires_force_and_preserves_local_edits`、`test_symlink_reinstall_syncs_injection_and_uninstall_retains_user_rules`、`test_unmanaged_conflict_does_not_gain_instructions`）到 Loom 测试的逐项对照：每项写明目标测试名与断言差异；按设计不再适用的行为写明理由和替代保护。对照未记录前，M2b 状态为“功能通过、等价记录待补”，不计入 G2。

### M2c：CLI/TUI/ng 兼容与分发

依赖：M2b；不阻塞桌面 G1。

| 任务 | 文件范围 | 可验证产出 |
|---|---|---|
| M2c-01 | `src-tauri/src/bin/loom-skills.rs`、Cargo、`tools/skills-cli/` | 无窗口 CLI 调共用服务；默认 binary 明确，普通 Tauri 构建仍成功 |
| M2c-02 | skills-linker 参数适配、golden fixture | tools/list/install/uninstall/status/doctor、category/scope/target-dir/skills-root、JSON/退出码；CLI 默认 symlink、桌面 copy |
| M2c-03 | tui/ng 包装层 | manual 选择、预览、快捷触发文本和重复操作；fzf 缺失可诊断，不在后台自动安装；显式 `--auto-install-fzf` 的去向在 M0 兼容表说明 |
| M2c-04 | 构建/打包配置、使用文档 | 开发源码入口和随包 CLI 的明确定位/调用方法；移动 `.app` 后可运行，未设置 PATH 也有完整调用路径 |
| M2c-05 | 双入口对照、coordination、更新准入与跨进程验证 | 桌面/CLI 状态一致；实际 CLI 使用共享生命周期 lease，活动时更新延后，更新期间新 CLI 拒绝，失败后恢复；另一 dataDir 的 CLI 可发现待恢复事务；无隐藏窗口或另一套安装实现 |

`--force` 等旧行为不能被“完全兼容”掩盖安全变更：M0 明确哪些操作仍允许显式替换、哪些旧的未托管删除被拒绝，并更新帮助和测试。迁移期原脚本可保存为 fixture/参考或隔离环境的兼容入口，不与新服务同时管理同一目标；M2c 出口要求 Loom 内只有共用 Rust 服务及薄交互入口；源仓库旧实现保持 v2 写入保护并用于兼容对照，统一在 G2 后的 M7 删除，避免与旧数据接管顺序冲突。

出口：CLI/TUI 功能对照和实际打包路径通过；功能所在仓库已转到 Loom，不依赖旧 skills checkout。

### M3：翻译与交互功能迁移

依赖：M1a；集成验收要求 M2a，独立于 M2c。

- [ ] 迁入四个已注册 provider、模型/语言选择和 summary/markdown；配置进入 Loom Settings 的 Skills 分区。
- [ ] 本地 provider 复用 CLI 探测与受控进程执行，使用独立操作上下文；不复用用户 Chat 会话和原生续聊句柄。
- [ ] 迁入复制、分享文本、命令面板及详情相关文件展示；不自动外发技能内容。
- [ ] 消除旧应用全局 CSS、document 主题写入、嵌套设置和默认 mock；校验 React 18 兼容性。
- [ ] 失败、取消、不可用 provider、快速切换技能和项目都显示正确归属的状态。

产出：功能矩阵中剩余 UI 能力具备 Loom 实现。出口：provider 请求/响应 fixture 和交互验证通过；真实 provider 测试状态单独列出。

任务拆分：M3-01 provider 配置与 secret DTO（已接 Settings → Skills，验收见 §9.2）；M3-02 四个翻译实现及超时/取消；M3-03 UI 竞态、复制和命令面板；M3-04 本地 Agent 与远程 provider 的真实验收。M3-01/02 需要配置与错误 fixture，M3-03 需要切页/切技能回归，M3-04 逐 provider 回填，不用一个成功结果代表四个 provider。

### M4：旧数据迁移与回滚

依赖：M1b、M2b、M3 的路径、schema 和功能稳定；无须等待 CLI 分发完成才能开发迁移器。

- [ ] 实现迁移发现、预览、快照、暂存、提交、重试和恢复，遵循第 5 节。
- [ ] 覆盖旧 app data、显式数据目录、repo cache、旧 bundle symlink、JSON/TSV 清单、配置、缺失来源和目标冲突。
- [ ] 在合成 HOME/项目中执行迁移 → 重启 → 重复迁移 → 卸载 → 回滚演练，检查未托管文件和用户新增修改。
- [ ] 增加取消/异常退出/磁盘写入失败场景，不把部分导入标为迁移成功。
- [ ] 以真实旧数据的受控副本完成迁移验收，报告脱敏；未完成该项时不得宣称用户数据迁移通过。

产出：可恢复的迁移入口和数据兼容报告。出口：旧记录可识别、链接有效、重复导入无副作用、回滚不覆盖后续修改。

任务拆分：M4-01 发现/只读预览和 alias；M4-02 备份/staging/凭据导入；M4-03 旧清单与共享注入块使用者核对及 cache/bundle 链接固定，无法确认归属时保留块并报告；M4-04 经共用事务索引和更新准入协议应用、幂等重试、逐项恢复；M4-05 合成故障矩阵与真实副本演练，补迁移/回滚期间的更新延后验证。每项修改 `skills/migration.rs` 及必要共用模块；M4-05 出口前，真实旧安装保持只读，不批量接管。

### M5：技能内容工具脱离 Manager

依赖：M0 的文件映射；内容工具移动可先执行。M5-02 依赖 M2b 等价对照：Loom 已有等价测试后，内容检查入口（`scripts/skills-quality-check` 与内容测试发现）不再运行管理测试即可关闭；源管理测试文件不在 M5 删除，随旧应用在 M7 删除，避免 G2 依赖 M7。

- [ ] 在 skills 仓库迁移第 3.3 节的内容检查、routing 工具和测试，更新相对路径、`support.py` 与测试发现入口。
- [ ] 更新 README、AGENTS、skill-authoring、技能中引用的管理命令；需要安装时指向 Loom CLI，内容检查不依赖该 CLI。
- [ ] 比较迁移前后受版本控制的技能及归档文件哈希；除必要路径文案外，技能内容不变，归档字节不变。
- [ ] 从没有 Loom checkout、Node workspace 的干净技能仓库副本运行 `bash scripts/skills-quality-check`。

产出：独立内容仓库工具链。出口：元数据、归档完整性、routing 管道和技能自有测试通过；无 Manager 运行依赖。

任务拆分：M5-01 移动工具及 support 导入；M5-02 内容检查入口排除管理测试并引用 Loom 等价对照（不删除源管理测试）；M5-03 文档与命令引用；M5-04 干净内容仓库检查与哈希对照。每次移动先修调用方再删除旧入口；过渡包装脚本必须有在 M7 移除的清单。

### M6：完整替代验收与切换（G2）

依赖：G1 与 M1b、M2b、M2c、M3、M4、M5；这是删除旧应用的硬门槛。

- [ ] 按第 7 节执行完整门禁、真实桌面、无源码目录打包启动和 CLI 分发路径验收。
- [ ] 验证真实 CLI 技能发现/显式调用，分别记录文件已安装、CLI 可发现、真实调用三个层级；能力缺口不得用 UI 状态替代。
- [ ] 验证 Chat 流式期间切页、切项目、草稿、停止/续聊、重启与 Settings；不改旧 Task 数据。
- [ ] 核对功能矩阵全部必需项及真实翻译/来源路径；受环境阻塞的能力标 blocked，G1 可保持通过，G2 不得通过或把整体迁移写成完成。
- [ ] 为目标交付记录可恢复源码版本、构建产物位置及验收证据；旧管理器停止写入后切到 Loom 管理。

任务拆分：M6-01 全量工程与打包；M6-02 真实功能矩阵及 V-16–18 的实际入口集成验证；M6-03 真实副本迁移与回滚证据复核；M6-04 用户旧入口切换与双仓交付记录。出口：成功标准 S1–S8 通过，即 G2，通过后允许 M7；公开发布额外遵循 Loom 发布门禁。

### M7：移除源管理应用与文档收尾

依赖：M6/G2 验收通过，并具备目标交付可恢复版本。

- [ ] 按映射逐项删除 skills 的旧应用、专属包、测试（含 `test_managed_instructions.py` 等管理测试）、启动/打包脚本及无消费者依赖；禁止直接 `rm -rf apps` 跳过核查。
- [ ] 扫描活动源码/脚本/文档中的 `skills-manager`、`skills-linker`、旧 `apps/` 引用，逐项区分残留依赖与合法迁移说明/兼容 marker。
- [ ] 更新源仓库 README、AGENTS 和迁移说明；保留质量入口与归档。旧历史链接按重定位或明确退役处理。
- [ ] 从干净 skills checkout 再跑内容质量检查；从不含 skills checkout 的环境运行 Loom 构建/启动验证。
- [ ] 回填 Loom 需求、架构、测试指南、操作文档、计划索引与验收记录，记录两仓最终提交及全部例外。

产出：skills 仅保留内容与质量工具，Manager 功能由 Loom 独立承载。出口：S9 通过，且无本次迁移引入的悬空文档/脚本依赖。

任务拆分：M7-01 文件级删除与消费者复核；M7-02 去除 workspace/专属依赖；M7-03 双仓文档和旧命令清理；M7-04 干净 checkout 双向独立检查和最终证据。删除前记录源可恢复 Git 状态与必要未提交补丁；只留 HEAD 不足以恢复未提交的源修改。

## 7. 成功标准与验证策略

### 7.1 两个交付门槛

| 门槛 | 必须通过 | 允许的交付 | 未通过时的动作 |
|---|---|---|---|
| G1 核心可用 | M0/M1a/M2a 出口；核心契约、锁/快照/恢复、V-17 跨客户端事务发现、V-18 桌面准入与测试进程 lease、真实桌面及至少一个真实 Agent CLI 发现/调用；`pnpm check` 与 no-bundle 构建 | Loom Skills 的本地/Git 来源、浏览、安装/更新/卸载；能力分阶段可见 | 停留在隔离 fixture/预览，不写入真实既有安装；修复失败项 |
| G2 完整替代 | G1 + M1b/M2b/M2c/M3/M4/M5；S1–S8 全部完成；保留能力各自有对应证据 | 正式切换管理入口，执行 M7 删除旧管理应用源码 | 保留已通过的 G1 交付；只阻塞相关能力与 M7，继续独立任务 |

G1 的范围声明不是功能删减：CLI/TUI、翻译和迁移仍在本计划必交付范围。没有凭据的 provider 标 blocked，不宣称功能已验证；也不因此撤销已完成核心功能。保留旧源码期间，旧/新管理器不得同时写同一安装目标。

完成 G2 后 M7 还需通过 S9，才能把整个迁移标为完成。需要缩减保留能力时，应明确更新需求、功能矩阵和此门槛，不能在验收失败后自行降低标准。

### 7.2 最终成功标准

| 编号 | 可验证结果 |
|---|---|
| S1 | Loom 可独立构建和启动；空库及配置外部本地来源可用，不依赖原仓库或旧 `.app` |
| S2 | 浏览、来源、安装、doctor、wrapper/注入、翻译及 CLI/TUI 功能矩阵完成，无静默退化 |
| S3 | 全局/项目、自定义目标、copy/symlink、alias、冲突与卸载通过；共享块保留全部使用者仍需的内容；远程刷新不隐式更新安装 |
| S4 | 旧数据迁移、重启、幂等重试、失败恢复及回滚通过；跨 dataDir 客户端能发现未完成事务并先恢复/拒绝写入；有效链接与已安装内容不丢失 |
| S5 | Skills 与 Chat 相互隔离；草稿、流式、停止、续聊、主题/语言和项目归属正确 |
| S6 | 进程/路径/凭据边界、跨进程写入和桌面/CLI 与应用更新准入验证通过，无 secret 进入前端、日志或测试产物 |
| S7 | skills 内容质量工具独立可运行，技能与归档保全，routing smoke 不冒充模型评估 |
| S8 | Loom 全量工程门禁、目标平台桌面/CLI 分发路径与代表性真实使用验收有基线证据 |
| S9 | 旧 Manager 应用代码、专属依赖与运行入口从 skills 移除；双仓不依赖彼此源码运行 |

### 7.3 检查与场景

- **增量检查**：按 [验证矩阵](../../testing.md#开发变更验证矩阵) 运行相关前端单测/build、Rust fmt/定向 test/适用 Clippy；IPC 双端契约、权限与持久化变更须补失败恢复。
- **里程碑门禁**：`pnpm check`、`pnpm tauri build --no-bundle`；不运行冻结内核作为默认门禁。多 binary 引入后确保命令仍指向 Loom 桌面应用。
- **内容仓库**：迁移前 `bash apps/scripts/skills-quality-check`，迁移后 `bash scripts/skills-quality-check`；命令在各自源仓库根执行。
- **预览生命周期**：启动前 `scripts/debug.sh stop`；桌面 `scripts/debug.sh desktop`，Web `scripts/debug.sh web`；固定 1420，验证后 `scripts/debug.sh stop`。不并行遗留旧 Manager 预览进程。
- **UI**：扩展 `pnpm smoke:visual`，覆盖中英文/深浅主题/窄窗口、空库/加载/错误、搜索选择、安装和翻译竞态、Chat 活动回合切换。截图放 `designs/exports/skills/`。
- **安装与安全 fixture**：临时 HOME 和项目；同名不同来源、无效 frontmatter、路径逃逸、目录 symlink、坏链接、缺失源、修改过的 copy、恶意 metadata、部分失败、并发 UI/CLI、Codex/Amp 共享 AGENTS、注入 marker 损坏及卸载源已删除场景。
- **来源/翻译 fixture**：本地 Git 仓库与合成 HTTP/provider 响应；超时/取消、认证失败、rate limit、truncated tree、默认分支变化、provider 未配置、错误输出和 secret 脱敏。
- **迁移 fixture**：旧 JSON/TSV、重复 ID、重叠来源、旧 cache/bundle symlink、权限失败、损坏 JSON、半完成迁移、幂等重试和回滚遇到新修改。
- **真实验收**：真实 GitHub/GitLab 来源读取与代表性 HTTPS/SSH 路径；GitHub API 单独验证；四个翻译 provider 逐项记录；Codex/Claude 的技能发现与显式调用、Amp 安装路径按实际支持情况验收。需要账户的动作先验证个人身份；有费用或当前权限不足时记录阻塞，不用 fixture 充当真实通过。G1/G2 分别记录，不因可选于 G1 的能力被阻塞而撤销 G1。
- **打包**：把构建产物移到独立临时位置，以隔离目录运行；验证无旧源码、重启、来源库、目标目录、CLI 和缓存。无 bundle 构建不等于签名、公证或公开分发验收。

证据保存到实施当日的 `docs/dogfood/YYYY-MM-DD-skills-migration.md`；记录两仓 SHA、未提交差异、命令、退出码、环境、产物位置及 passed/failed/blocked/not-run。预期路径在实施时创建，不将本计划作为已通过证据。

### 7.4 必须落到测试或操作记录的关键用例

| 用例 | 场景与断言 | 对应任务/门槛 |
|---|---|---|
| V-01 | 两个进程安装到同一 target，清单无丢项；数据目录不同也争用同一锁；持锁进程被终止后可恢复 | M0-05/M2a-01，G1 |
| V-02 | Codex 与 Amp 同时写同一项目 AGENTS，两个托管块均存在、用户正文不变；反向操作无死锁 | M2b-03 用测试进程；M2c-05 用真实入口；G2 |
| V-03 | revision A 安装后刷新 B；安装仍是 A，显式更新才是 B，回滚可恢复 A | M1a-02/M2a-03，G1 |
| V-04 | B 中技能删除/改名、拉取失败、取消刷新；A 链接仍有效，失败不发布不完整快照 | M1a-02/M2a-03，G1 |
| V-05 | copy 被用户修改；更新/卸载前识别差异，默认不覆盖/删除，显式处理结果记录 | M2a-02/03，G1 |
| V-06 | 技能文件/manifest/wrapper/注入各提交点崩溃；重启恢复或明确 recovery_required，无假成功 | 文件+manifest 在 M2a-04/G1；wrapper+注入在 M2b-04/G2 |
| V-07 | SRC-01–10 按适用阶段完成；GitLab namespace、SSH、旧 group/skill ID 不被 GitHub 逻辑破坏 | M1a/M1b/M4；完整矩阵 G2 |
| V-08 | 数据目录+旧清单+cache/bundle 链接迁入；重复执行无新增副作用，dirty cache 内容不丢失 | M4-03/04/05，G2 |
| V-09 | 迁移后用户改文件再回滚；只恢复匹配本次后指纹的项，其他项保留并报告冲突 | M4-04/05，G2 |
| V-10 | Chat → Settings → Skills，设置侧栏可见、Chat 无同级 Skills 按钮；流式期间切换设置板块与返回 Chat 不串流/丢草稿；安装目标保持发起时归属 | M1a-04/M2a-05，G1；M6 回归 |
| V-11 | CLI 与桌面列表、目标、安装/卸载语义一致；`--force` 的安全差异与 JSON/退出码有黄金样本 | M2c-02/05，G2 |
| V-12 | 翻译取消/超时/切技能/凭据缺失，结果不串写；日志与前端无 secret | M3，G2 |
| V-13 | 移动构建产物到独立目录，无相邻源码可启动；随后随包 CLI 在无 PATH 配置时可调用 | 桌面在 G1；CLI 在 M2c/G2 |
| V-14 | 干净 skills checkout 无 Node/Tauri/Loom 依赖运行内容检查，归档哈希不变 | M5，G2；M7 再验 |
| V-15 | 删除源应用后，双仓各自构建/检查无旧路径依赖；合法 marker/迁移历史引用不误删 | M7，S9 |
| V-16 | 同一 auto skill 安装给 Codex/Amp，卸载一个仍保块，最后一个才删除；重复安装不增加重复使用者，同 marker 内容变化不覆盖其他使用者；迁移未知归属不误删 | M0-03 定义；M2b-01/03/04 实现与恢复；M4-03 迁移；G2 |
| V-17 | A 替换文件后崩溃，另一 dataDir 的 B 先发现共用标记，恢复或拒绝重叠写入；缺日志/损坏索引不当空状态；committed 残留只收尾 | M0-05/M2a-04，G1；M2c-05 实际 CLI 及 M6 回归 |
| V-18 | 无子进程的文件操作中更新延后；更新准入后新桌面/CLI 操作拒绝；失败后恢复入口；外部 CLI 活动时更新延后；跨进程锁顺序和崩溃解锁正确 | M2a-06 的桌面/测试进程，G1；M2c-05 实际 CLI、M4-05 迁移/回滚、M6 集成，G2 |

首批建议测试文件：`tests/unit/skillsProjection.test.cjs`、`tests/unit/skillsInstallPanel.test.cjs`；Rust 按 catalog/sources/installers/instructions/migration 模块组织定向测试，跨进程锁与 CLI 用例放集成测试。名称可依仓库测试发现机制调整，但 V 编号需在证据中可追溯。

## 8. 风险、待确认事项与执行规则

| 风险 | 应对 |
|---|---|
| 源 CLI 与桌面同名能力不等价 | 先做并集矩阵和兼容 fixture，明确模式/force/marker 语义后统一实现 |
| React、CSS、状态迁入影响现有 Chat | 组件级迁移、CSS 作用域、懒加载与真实切页回归；不整包嵌入旧 App |
| 缓存刷新/搬迁、旧 bundle 卸载造成安装突变或链接失效 | 固定 revision 快照、刷新与显式更新分离、引用保护和旧内容迁移 |
| 同项目不同 Agent 并发修改共享 AGENTS | 用户级共用 writer 锁覆盖完整 write set，提交前指纹复核；旧 Manager 停写 |
| 同技能被多个安装使用时卸载误删共享块 | canonical 文件/marker 的使用者集合、最后使用者删除和内容冲突规则；旧归属不明时保留 |
| A 崩溃后 B 无法发现另一 dataDir 的半提交 | 固定协调目录 PendingTransaction，写前持久化、重叠写入先恢复、缺记录内容时保守阻止 |
| 纯文件事务或外部 CLI 活跃时应用安装更新 | 进程内 operation guard 与跨进程生命周期 lease；更新独占准入、固定锁顺序和失败释放 |
| 删除 apps 误伤内容工具和技能资源 | 文件级去向表、独立质量门禁、技能/归档哈希核查后删除 |
| provider 缺少配置或真实 CLI 能力未知 | G1/G2 分开；相关项标 blocked，保留核心交付，不降低最终退役标准 |
| 双仓工作区存在其他在途改动 | 实施前复核状态，分离本任务补丁，保留用户已有变更；不以旧 SHA 覆盖当前工作区 |

当前没有必须先向用户确认才能启动 M0 的产品问题。以下默认选择已写入本计划：保留 CLI/TUI/ng 与翻译功能、退役独立 Web/API 壳、默认不内置个人技能集合。M0 负责以源码样本固化 CLI 安全差异、schema、共享块使用者、跨客户端恢复与更新准入契约；这是实施任务，不是额外审批。若用户后续明确要求独立 Web 产品或删减某项能力，先更新功能矩阵和验收门槛，再调整实现。

用户已授权按本计划实施、测试并阶段提交，两仓修改持续推进。可逆的已授权本地修改无需重复确认；跨仓库写权限、外部账户操作和实际运行环境审批按当时权限处理。该授权不包含发布、外发技能内容或删除个人数据。

## 9. 实施与验证记录

### 9.1 实施就绪审查的处理结果

| 审查项 | 计划变更 | 关闭条件 |
|---|---|---|
| P1 共享指令锁缺失 | 第 5.3 节定义共用 writer 锁；M2b-03/V-02 覆盖跨 Agent AGENTS | 计划缺口已补，运行正确性待测试 |
| P1 刷新隐式改变 symlink 安装 | 第 5.4 节固定 revision，目录刷新与安装更新分离；V-03/04 | 计划缺口已补，快照/回滚待实现 |
| P2 M2 同时迁移/重写/分发 | 拆 M2a/M2b/M2c；增加 G1/G2，GitHub API 另列 M1b | 桌面核心无需等待全部兼容实现 |
| P2 GitLab 能力未显式覆盖 | SRC-02–10 与 M1a/M1b/M4 明确 URL、transport、namespace 和旧身份 | 实际兼容待对照样本与真实验收 |
| P1 共享块缺少使用者归属 | v3 第 5.5 节、SharedInstruction；M0-03 固化契约，M2b-01/03/04 实现与恢复，M4-03 核对旧归属；V-16 | G2 前通过部分卸载保块、最后使用者删除、重复安装幂等、内容冲突拒绝覆盖、失败恢复和旧归属不明保留；证据同时核对正文、使用者集合及安装清单 |
| P1 跨客户端无法发现未完成事务 | v3 第 5.6 节、PendingTransaction；M0-05 探针、M2a-04 恢复、M2c-05 实际 CLI、M6 集成；V-17 | G1 前通过 A 崩溃后另一 dataDir 的 B 恢复或拒绝重叠写入，缺日志/损坏索引拒绝写入，committed 残留只收尾；G2 补实际 CLI 入口证据 |
| P2 应用更新准入缺少 Skills 验收 | v3 第 5.7 节；M2a-06 桌面与测试进程、M2c-05 实际 CLI、M4-05 迁移/回滚、M6 集成；V-18 | G1 前通过纯文件操作期间更新延后、更新期间拒绝新操作、更新失败恢复准入及跨进程 lease；G2 补实际 CLI 和迁移/回滚证据，使用更新器替身验证 |

上述 v3 三项补充均已纳入正文契约、实施任务和验证矩阵，属于必需验收项。实施与 Review 时按以下对应关系核对：

- **共享注入块归属**：§5.5 → M0-03、M2b-01/03/04、M4-03 → V-16；检查正文、使用者集合和安装清单一致。阻塞 G2，不阻塞不提供自动注入的 G1。
- **跨客户端事务发现**：§5.6 → M0-05、M2a-04、M2c-05、M6 → V-17；不同 dataDir 的客户端必须先恢复或拒绝重叠写入。服务级场景阻塞 G1，实际 CLI 集成阻塞 G2。
- **应用更新准入**：§5.7 → M2a-06、M2c-05、M4-05、M6 → V-18；覆盖纯文件操作、外部 CLI、更新失败与迁移/回滚。桌面及测试进程场景阻塞 G1，实际 CLI 和迁移集成阻塞 G2。

计划补充完成不等于运行验收通过；按第 9.2 节与实施证据逐项回填，不以本文中的验收要求代替通过记录。

“可从 M0 开始”只表示计划已具备执行入口，不表示探针、迁移或任何应用验收已通过。

### 9.2 当前记录

| 项目 | 状态 | 证据 |
|---|---|---|
| 代码与文档盘点 | 已完成只读检查 | 第 2 节源码入口与本文件基线；未运行应用 |
| M0-01/02/06 | 已执行 | 两仓基线、需求/架构/安全边界更新；源基线测试修复提交 `349d622`，完整检查通过 |
| M0-03/04/05 | 基础与样本已落地，安装集成待后续阶段 | Skills DTO 双端样本、旧 library/JSON/TSV 合成 fixture、lease/writer/pending 和不可变快照探针；完整安装归属/恢复属于 M2/M4 验收 |
| M1a-01/02/03/05 | 来源切片通过离线/本机验证 | catalog/repository/snapshots/operations 与 8 个 IPC 已接线；22 项 Skills 测试含真实本地 Git clone/refresh、禁用来源 smudge、取消与重启记录；外网 GitHub/GitLab 实测待后续（测试/IPC 数为该切片当时计数，当前总数见“应用测试与构建”行） |
| M1a-04 | 已按用户补充要求改为设置内板块 | 顶层仍 Chat/Settings；Settings 侧栏 SkillsPanel 原生验证本地来源添加、详情/文件、搜索/刷新、重启、移除保留文件、返回 Chat 保留草稿，中英文/深浅主题观察通过 |
| M2a 安装闭环 | 已实现并分项验证，待 G1 综合关闭 | 16 个 Skills IPC、v2 清单、copy/symlink/显式更新/卸载、跨 dataDir 事务恢复、安装与 updater lease 接线；原生 5 次规划、真实 Codex 发现/调用通过；备份避开技能发现且 Git 忽略 |
| M2b-01/02/03/04 | 功能与 8 项等价对照通过；旧归属迁移归 M4 | auto/global、manual wrapper、只读诊断、使用者 ledger、跨文件事务及多使用者一致更新已接线；独立进程并发、整组 12 个 rename 故障点恢复、原生 A → B 一致更新/卸载保全、临时 HOME 的全局路径通过；真实 CLI 接线按 M2c 复验 |
| M2c-01–05 | 当前保留能力出口通过 | CLI/TUI/ng 同一 Rust 服务；真实 fzf 预览/重复安装卸载、自定义含空格/中文路径取消、搬移后的启动器、25 项真实目录身份/分类对照、CLI → 桌面 → CLI 交接、另一 dataDir 的真实 CLI 恢复通过；fzf 显式安装使用受控替身，未下载真实软件；旧源实现仍按 G2/M7 退役 |
| M5-01/03/04 | 内容独立化切片通过 | 源提交 `fd7a4ec`；隔离副本无 apps/Node/Tauri/Loom 的 32 项内容测试、7 项旧管理测试通过，272 个技能/归档文件哈希不变 |
| M5-02、M5 总出口 | 内容独立出口通过 | 8 项源管理测试已逐项映射目标测试与安全/doctor 差异；根内容入口不运行管理测试。新隔离副本 32 项内容测试、326 个管道用例通过；271 个受管技能/归档哈希不变（原 272 记录含未受管 .DS_Store）；源管理测试与旧转发入口仍留到 M7 |
| 其余实施；G1/G2 | 未完成/未验收 | G1 已补真实本地 Git 的固定安装版本全链与更新器失败准入回归，仍待视觉矩阵、无源码访问验收、外网 GitHub/GitLab 实测；M3/M4、M6/G2 与 M7 源应用退役待完成；M1b API/SSH 与 M2b/M5 本轮证据见 2026-10-22 验收记录，详见实施证据 |
| M3-01 | 配置切片通过 | 4 provider 配置、显式 key 变更/脱敏 DTO、并发 revision/锁与 pending、18 个 Skills IPC；完整 check 前端 115/Rust 251，随后表单草稿保留回归使前端 116 通过，最终 release app bundle 通过；原生假 key 保存/重启读取/清除/重新加载与 0600 权限通过；模型调用未实施 |
| M1b-01–04 | API 切片与 SSH 配置兼容通过 | 固定 commit 完整包、默认分支/旧模式映射、错误/限流/取消及 SSH 配置 7 场景；真实匿名 API 与原生临时安装/来源移除后卸载通过；私有认证及外网 Git clone 另按矩阵验收 |
| v5 本轮工程门禁 | 通过，整体未完成 | `pnpm check` 前端 111/Rust 246（23 项忽略）；最终 release app bundle 通过；源测试 8 项对照和内容隔离证据见 [本轮验收](../../dogfood/2026-10-22-skills-migration-review.md) |
| v4 进度审查修订 | 已修正计划 | 依据 `0b63aad` 两轮审查：M5/M7 依赖、M2b 等价对照要求、状态行、§4 实际模块表、M2a-01/04 文件范围、SRC-09、本表过期行；仅文档修改，未运行应用测试 |
| v3 文档一致性、相对链接、索引排序 | 通过 | 链接/锚点、索引唯一性与倒序、章节引用、11 阶段/50 任务、SRC-01–10/V-01–18、代码围栏及空白检查通过；人工核对三项补充的契约—任务—验证—门槛对应关系，`git diff --check` 通过 |
| 应用测试与构建 | 分项通过，详见证据 | M2c 收尾完整 check：前端 109/Rust 230；真实 fzf 和真实 CLI 恢复两个 opt-in 检查另显式通过；release no-bundle 与 debug app bundle 通过；release app bundle、搬移后独立 cwd 原生运行通过；旧 Manager 版本保护后完整 check：应用 99/Rust 25/内容 32/管理 8 |
| 安装/真实服务/迁移验收 | 安装分项通过，整体未完成 | 临时项目 copy/更新/卸载与孤立 symlink 卸载、真实 Codex 调用通过；外网来源、真实翻译 provider、旧数据迁移和整体 G1/G2 未验收 |

实施每个里程碑后更新本表及 [计划索引](../../PLANS.md)，成功标准逐项回填；不以“代码已迁入”代替迁移完成。
