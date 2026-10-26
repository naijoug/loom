# Skills 契约与实施状态

基线：Loom `c8313dd`；源 skills `a245966`。执行入口为 [迁移计划](../plans/2026-10-21/06:12-skills-module-migration.md)。本文件区分已落地基础与后续实现约定，不作为功能交付证明。

## 当前实现

- `src-tauri/src/skills/{error,files,coordination}.rs`：有类型错误、私有/有界/原子元数据、跨进程生命周期 lease 和 writer 锁、带版本 pending 索引。
- `sources.rs`：GitHub/GitLab URL 与 transport 标准化、旧 ID alias、输入校验；`operations.rs` 已接受控 Git clone/checkout，真实本地 Git fixture 覆盖导入、刷新、保留旧版本与来源过滤脚本禁用。
- `catalog.rs`、`repository.rs`：本地目录/根技能/嵌套技能、详情/相关文件、逐项错误、来源登记及重启读取。前端入口为 Settings → Skills，沿用设置侧栏及 Chat/Settings 两个顶层视图。
- 业务服务不依赖 AppHandle、窗口或 ChatSession，构造时必须传明确协调根目录；`commands.rs` 是负责主窗口校验、路径解析与事件转发的 Tauri 薄层。安装与恢复命令已接 supervisor operation guard，updater 已接跨进程独占 lease；`loom-skills` 已直接调用同一服务，无窗口；CLI 项目与自定义目标由参数显式授权，桌面仍按登记项目校验。
- 八项定向测试包含两个真实子进程场景：杀死持锁进程后锁释放；子进程不运行析构直接退出后，新客户端仍看到 pending。缺日志、坏 JSON、未知 pending schema、路径穿越和 symlink 也有覆盖。
- `package.rs`、`transactions.rs`、`targets.rs`、`installers.rs` 已实现完整包指纹、目标枚举、预览/执行、逐项提交/回滚与受校验的 pending 恢复。真实子进程异常退出后由另一 dataDir 恢复有测试；`instructions.rs` 已接共享指令使用者集合、manual wrapper、只读诊断与跨文件恢复。

## 路径与锁

生产路径解析：库默认位于 Loom app data 的 `skills/`，`LOOM_SKILLS_DATA_DIR` 可指定绝对库目录以隔离验收或迁移；未指定时可使用 `LOOM_PROFILE_DIR/skills`，否则使用默认 app data/skills。绝对 `LOOM_PROFILE_DIR` 同时隔离 settings/agents/recent-projects；固定协调目录始终为同一 OS 用户的 Loom app data 下 `skills-coordination/`，不受自定义库目录影响。测试协调目录仅经服务依赖注入提供。

`Coordinator::enter` 获取共享生命周期 lease；`Coordinator::writer(&lease)` 获取短时独占 writer。WriterGuard 借用 lease，禁止生命周期提前结束。`try_update` 非阻塞获取独占生命周期 lease，并检查 pending；存在活动操作/待恢复事务时失败，RAII 释放准入。

顺序：生命周期 lease → supervisor operation/update guard → writer。安装/恢复和 updater 已遵循此顺序；纯文件工作计入 supervisor。更新入口的 `with_install_admission` 与安装器替身共用，可验证安装失败和任一层准入失败均释放已取得的 guard。短时 writer 争用最多等待 500ms；取消只写幂等控制标记并持 lifecycle lease，不等待 writer，因此文件提交期间仍可请求取消。提交临界区安全收口后响应取消。

## 元数据与事务

- 私有元数据最大 8 MiB；读坏/超限明确失败。原子写在相同目录 staging，先文件 `sync_all`，rename 后同步父目录（Unix）。这不是多文件原子提交。
- PendingTransaction v1：operationId、canonical writeSet、dataDir、journalPath、preparedDigest。journal 必须位于 dataDir 内；准备日志先持久化，发布前检查 digest 未变。
- 索引位于固定协调目录 `pending/<operationId>.json`；严格 schema/身份验证。重叠包括同一路径及父子目录；损坏索引阻止新写入，不相交且可判定的未完成资源不阻止无关写入。
- `transactions.rs` 保存不可变 prepare 日志与独立 committed/rolled_back 回执，核验实际资源后清除 marker；不能因锁已释放或 PID 消失清除。用户在崩溃后编辑的内容和缺失备份均导致拒绝自动回滚，保留待恢复状态。
- staging/backup 位于目标 skills 目录的同级 `.loom-skill-transactions/<transactionId>/`，必须同文件系统；避开 Agent 技能发现目录，并用该目录内 `.gitignore` 忽略备份。备份保留供恢复，回执展示位置，不自动 GC。
- 操作/日志不含凭据、完整环境或用户技能正文；必要备份仅存本机受限目录，错误输出不回显解析原文。

## 安装与共享指令

目标目录的 `.skills-linker-manifest.json` 是安装归属权威记录。新写入采用 version 2，记录 installationId、skillId、sourceId、relativePath、installName、sourceDir、mode、revision、sourceDigest、installedState、时间戳和 managedResources；本地 symlink 的 revision 为空，UI 标记 live；v1/TSV 只读识别并拒绝写入，不提供接管或升级。旧 Manager 数据迁移已取消；Loom v2 的事务恢复仍保留。

安装规划有效期 10 分钟，捕获 source/目标/manifest 指纹和项目上下文；执行前复核，拒绝过期或目标变化。重复执行读取已有回执；同批多个安装共用 manifest 时使用前一提交后的校验状态。未托管同名内容禁止覆盖，用户修改过的受管 copy 需显式替换并保留备份；卸载修改过的内容默认拒绝。移除来源登记后仍可按 installationId 卸载。

managedResources 记录 canonical path、marker 和 contentDigest。SharedInstruction v1 保存在固定协调目录的 `shared-instructions/<resource-key-digest>.json`，键为 canonical 指令文件路径与 marker；保存资源类型、内容指纹和 installationId → manifestPath/installName 使用者集合。每次变更核对所有使用者的清单引用，缺失或无法读取不能当成零使用者。相同内容可共享，最后一个已核实使用者移除才删块；不同内容明确冲突。旧 marker 无使用者集合不得默认归当前安装。

`auto/global` 且有 inject.md 时生成与旧工具相同的 marker；cron 不注入。路径类别优先，其次 skill.yaml 的 category/activation。manual 的 `withSlashCommands` 默认为 false，Codex/Claude 可显式生成 wrapper，指向实际安装包路径，Claude 保留 `$ARGUMENTS`。未知 wrapper（包括空文件）不能因勾选覆盖策略而覆盖。源内容与用户指令中的嵌套/重复/异常 marker 被拒绝。

规划附带 resourceChanges 前指纹；执行阶段在同一 writer 锁下重算并比较。指令正文、wrapper、共享 ledger、技能文件和安装清单进入同一个 prepare journal；批量操作只沿用该批次已验证提交的后指纹。恢复既核对 journal digest，也限制为已授权安装目录、具体指令路径、命令文件和 ledger，拒绝借项目目录覆盖无关文件。跨目录 rename 同步两端父目录；prepare 失败但 pending 已可见时保留恢复材料。

`skills_get_install_status` / `skills_list_installations` 附带 instructionChecks：current/missing/stale/malformed；诊断不写文件，local-live 的 inject.md 变化会标 stale，需显式更新。前端规划显示实际受影响的指令文件，不展示内部 ledger。指令最终符号链接只在 canonical 目标仍处于获准 home/project 范围且不与技能包重叠时保留，否则拒绝。

选择的安装若共享指令路径，规划标记 `coordinated: true`。规划与执行均核对当前 ledger 的全部使用者；改变共享内容必须选齐使用者且最终正文一致，否则冲突。选中的共享安装（以及该规划中的其他可执行项）复用单目标的暂存/校验函数，以一个 `<planId>-group` prepare journal 提交技能包、各清单、去重后的指令文件和 ledger；同一文件只生成一个一致的最终内容。独立资源保持逐目标提交，卸载按使用者逐项移除。

组内任一目标/指纹变化，在变更前拒绝整组；提交失败按统一日志补偿，若用户在中断后修改内容则保留冲突并标待恢复，不报告部分成功。幂等重试读取同一组回执；应用更新仍由原生命周期 lease/operation guard 协调。这是统一恢复边界，不宣称跨文件系统瞬时原子性。CLI 核心命令与 doctor 已接共用服务；TUI/ng 同样调用该服务，旧归属不重建，未知归属继续拒绝写入。

远程来源快照固定 commit；dirty legacy cache 以内容 digest 单独标识，不能冒充上游 commit。刷新目录只更新 latestRevision，symlink 固定指向不可变路径。本地 live symlink 明示行为，不把其变化伪装成版本化安装。

## 来源、安装与前后端

当前已注册 list_library/get_detail/add_local_source/remove_source/import_repository/refresh_source/list_operations/cancel_operation 八个 `skills_` 命令，以及 `loom://skills-operation` 事件。`contracts/tauri-contract.json` 的 SkillsLibrary / SkillsOperation 样本在 Rust 与 TypeScript 双端校验。安装阶段新增 list_install_targets/get_install_status/list_installations/plan_install/plan_uninstall/apply_install/list_pending/recover_installation 八个 `skills_` 命令，共 16 个；InstallTarget/Installation/InstallPlan/InstallReceipt 同样有双端样本。M3 追加配置 2 个与翻译执行/历史/结果 3 个命令，当前总计 21 个；旧数据迁移命令已按用户要求撤下。

错误形态为 `{code,message}`；当前 code 为 invalid_input/conflict/lock_busy/updating/recovery_required/permission_denied/io/cancelled/timeout/source_unavailable/auth_required/rate_limited。前端展示错误 message，流程判断不依赖解析自然语言。

operationId 关联持久化进度、sourceId、序号、终态及脱敏执行记录；进程存活由 OS owner 锁判断，失去 owner 的 running 记录改为 interrupted，不自动重放。UI 订阅事件并补查操作列表，按序号忽略旧事件。切换设置 Skills 不卸载 Chat；详情请求绑定选择，过滤/移除后旧结果不显示。

扫描限制为 20,000 个目录项、32 层，文本预览单文件 256 KiB、相关文件最多 500 个/文本合计 1 MiB。远程快照最多 128 MiB；symlink、submodule 和未物化 LFS 明确拒绝，不把不完整包标成成功。元数据沿用旧 Manager 的 scalar/list YAML 子集并支持常见 folded description，不运行 YAML tag/alias。

Git 在独立进程组执行，有 120 秒单命令超时、取消、输出有界排流及退出清理。clone 不 checkout，之后以 `.git/info/attributes` 的高优先级禁用 filter/encoding 转换，再 checkout 固定 commit；hooks/fsmonitor/submodule 自动动作关闭。依据 [Git attributes 官方说明](https://git-scm.com/docs/gitattributes)，另有真实恶意 smudge fixture 验证没有执行。

来源库只做原子单文件更新；远程快照先完成并校验再发布，刷新只切换目录 latest revision。移除来源保留快照及用户原目录；显式安装更新经安装规划/事务执行；安全 GC 尚未实现。

## 源功能去向与兼容基线

| 源能力/目录 | 去向 | 首次交付 |
|---|---|---|
| core 解析/稳定 ID、桌面扫描 | Rust catalog/source 模块，保留旧 ID alias | M1a |
| GitHub/GitLab HTTPS/SSH、默认分支 | 固定 revision 的来源与快照 | M1a |
| Web GitHub API 只读来源 | Rust API 读取与完整包物化 | M1b |
| Node/Rust/CLI 安装实现 | 共用 Rust 安装事务服务 | M2a/b/c |
| auto/global 注入、manual wrapper、doctor | 共享资源归属与指令服务 | M2b |
| skills-linker、tui、ng | Loom tools + 无窗口 CLI | M2c |
| 4 个已注册翻译 provider、复制/分享文本 | Skills UI 与后端任务 | M3 |
| 旧 app data/cache/bundle links | 用户取消数据迁移，原样保留；无导入/接管入口 | 已退出范围 |
| metadata/routing/archive/who-am-i 内容检查 | 保留在 skills 的独立 scripts/tests | M5 |
| Web/API/桌面旧壳与专属依赖 | 完整替代验收后移除 | M7 |

CLI 默认 symlink、桌面默认 copy；Codex/ChatGPT 目标去重；`--force` 不绕过未知归属删除保护，安全收紧需在新 CLI help/迁移说明中明确。完整参数/JSON 对照与 provider 实测仍待实施，不能用此表声称所有兼容项通过。

## CLI 核心入口（M2c 当前范围验收通过）

`skills/cli.rs` 解析参数并映射旧 JSON 字段；`src/bin/loom-skills.rs` 只负责输出/退出码，未调用 Tauri run。桌面与 CLI 使用 `runtime_paths.rs` 统一库覆盖，底层 dirs 6 与 Tauri 使用同一实现；CLI 直接从编译时 app identifier 解析默认 app data。`--data-dir` 不改变协调根。

自定义目录仅通过 CLI 构造 `TargetContext::with_custom`；目录身份仍参与目标前指纹、安装 ID、共享资源授权与恢复，未新增前端任意路径写入口。普通工具的 scope 指令路径保持既有约定，unknown/custom 工具不推断指令路径。无窗口 CLI 已随 Tauri bundle 输出，默认桌面 binary 明确为 loom；命令、行为差异与使用方式见 [CLI 说明](../skills-cli.md)。

TUI/ng 的交互编排位于 `skills/cli/interactive.rs`，无独立文件安装器。fzf 使用固定参数、数字行身份和受控文本输出；默认环境中的执行绑定不继承。TUI 应用保存的 planId，`installers::apply_reviewed_batch` 仅以已验证 commit journal 的后状态推进同批后续规划的 manifest/指令前指纹，源/目标/归属决定不重算；预览原文不改写，外部编辑仍冲突。各技能规划仍有独立结果，共享同一块的多工具更新继续用组级事务。

`tools/skills-cli/{skills-linker,tui,ng}` 仅做进程转发，随资源打包到 `Contents/Resources/tools/skills-cli`。显式 fzf 安装通过监督器运行固定 Homebrew argv，记录开始/结束/退出码、超时与取消；Settings 能识别该操作为 CLI 工具安装。全程持 CLI 生命周期 lease。

## GitHub API 来源与 SSH 配置（M1b）

`github_api.rs` 接受 GitHub 来源，以独立 `github_api` kind/sourceId 保存模式与默认分支。同仓库 Git/API 两种模式不静默合并；`skills_import_repository` 的可选 sourceMode 为 `git` 或 `github-api`，省略仍是 Git。刷新按原 kind 分派，CLI 列表/安装保留 API 的仓库相对路径。

HTTP 仅访问固定 HTTPS `api.github.com`，禁用重定向，不跟随响应中的下载地址。默认分支只解析一次 commit，再读取该 commit 的树和 blob；二进制 base64 解码后验证长度与 Git blob SHA1（身份校验，不是单独的安全签名）。协议依据 [Git trees](https://docs.github.com/en/rest/git/trees?apiVersion=2022-11-28) 和 [Git blobs](https://docs.github.com/en/rest/git/blobs?apiVersion=2022-11-28)。API 版本为仍受支持的 2022-11-28。

只物化规范扫描器可发现的完整技能包，保留包内脚本/agents/references/二进制和可执行位；归档/隐藏/生成目录不作为技能入口。这修正旧 API 无差别列举 SKILL.md 的行为，不删除源仓库内容。树截断、意外分页、路径/大小写冲突、symlink/submodule、LFS 指针、blob 缺失或损坏均拒绝发布。上限为 20,000 树项、128 MiB 技能包、2,048 请求；连接 10 秒、单请求 30 秒、整次 300 秒，支持在请求中取消。网络期间不持 writer 锁，成功后复核来源预期 revision 并发布不可变快照。

认证仅读取后端 GITHUB_TOKEN/GH_TOKEN 环境，header 标记敏感；操作 requests 仅保存 method/url/时间/status/受控错误，不保存 header 或 body。UI 最多展示最近 50 条，同时显示总数。401/403 认证与主/次限流分别处理，不回显服务端错误正文。真实公开匿名读取已验，真实私有仓库认证未验。

Git 显式 SSH transport 保留 GIT_SSH_COMMAND、GIT_SSH、GIT_SSH_VARIANT 和有效 core.sshCommand。未配置环境通路时以受监督的 `git config --name-only --get-regexp` 只查询键名；仅确认用户未配置才补 `ssh -oBatchMode=yes`，配置读取错误即失败。不关闭 host key 校验。自定义 SSH 原始 stderr 不进入持久日志，只保存受控分类/退出码，避免 wrapper 打印秘密。

旧数据来源解析器与预览已撤下；当前只接受用户主动添加的本地/Git/GitHub API 来源，不读取或接管旧库。

## 翻译配置（M3-01）

Settings → Skills 的翻译设置通过 `skills_list_translation_providers` / `skills_save_translation_config` 访问 Rust `translation/config.rs`，两者只允许主窗口；Skills IPC 共 18 个。四个 provider 保留原 ID：openai、openrouter、codex、claude-code；Amp 未注册。此切片只完成配置，执行、取消与结果尚由 M3-02/03 接线，不声称四个翻译 provider 已验收。

配置位于 Skills dataDir 的 `translation-config.json`，schemaVersion 1、单调 revision、按 provider 存储；文件私有 `0600`，读写拒绝 symlink/损坏/未来 schema。保存先取得 lifecycle lease → operation guard → writer，检查 pending write set；expectedRevision 不匹配返回 conflict，不覆盖其他客户端的新设置。API key 变化明确为 keep/set/clear，model null/空白恢复默认；本地 provider 不接受 key 修改。

前端仅收到 configured、storedKey、keySource、model/storedModel 和环境覆盖状态，不返回 secret 值；输入 DTO 不派生 Debug/Serialize。临时表单值仅保存在组件内，保存后及关闭面板时释放，不写 localStorage。各 provider 分别保存，保存一个保留其他 provider 的未保存草稿。

远程 key 的优先级保持旧行为：OPENAI_API_KEY/OPENROUTER_API_KEY 优先于保存值；清除保存值不清除环境变量，界面明确提示。模型优先级为 LOOM_SKILLS_OPENAI_MODEL/LOOM_SKILLS_OPENROUTER_MODEL → 对应旧 SKILLS_MANAGER_*_MODEL → 保存值 → 旧默认 gpt-5/openai/gpt-5；不在迁移中静默换模型。本地模型可独立设置，默认交给 CLI；命令可用性复用 Agent 的 resolve_executable，仅表示发现安装，不代表登录可用。

后续请求实现以 [OpenAI 文本生成文档](https://developers.openai.com/api/docs/guides/text) 与 [Codex CLI 文档](https://learn.chatgpt.com/docs/developer-commands?surface=cli) 为协议参考；真实调用按 provider 单独验收。已有配置读取不触发外发、登录或付费请求，不提供旧配置导入。

## 翻译执行与结果（M3-02/03）

新增 `skills_translate`、`skills_translation_history`、`skills_translation_result`，共 21 个 Skills IPC。前端只提交技能 ID、详情指纹、provider、目标语言和 markdown/summary 模式；正文由 Rust 从已登记来源读取。详情的 contentDigest 包括正文、展示元数据与 manifest，版本不匹配返回 conflict。摘要保留标题、描述和 References，完整模式保留 Markdown；发起后固定输入与 provider 配置，不跟随来源刷新。

每次翻译创建独立 Skills operation，持有生命周期 lease/operation guard，沿用取消标记、进程监督和持久终态。`translations/<operationId>.request.json` 仅存来源/输入指纹、provider、模型、语言与模式；成功结果单独私有保存到 `.result.json`，只有对应 committed 操作才能读取。没有输出正文或凭据写入 operation 日志；译文是用户可查看的本机结果，不是日志。中断/失败不自动重新发送模型请求，已保存但未确认 committed 的结果不冒充成功。

OpenAI 使用固定 HTTPS Responses 端点，关闭 store、不提供 tools；OpenRouter 使用固定 HTTPS Chat Completions 端点，不提供工具。均禁用重定向，连接 10 秒、请求 180 秒，响应最多 2 MiB、译文最多 1 MiB；认证/限流/超时分类，不回显远端正文或自动重试。OpenAI 只接受 completed 的 assistant/output_text，OpenRouter 只接受 stop 的单条 assistant 文本，拒绝截断、拒答、工具调用与空结果。OpenRouter 协议见 [官方 Chat Completions 文档](https://openrouter.ai/docs/api/api-reference/chat/create-a-chat-completion)。

本地 Codex/Claude 复用可执行文件发现和监督器，各自在私有临时工作目录运行，正文仅经 stdin；不复用用户 Chat 会话、不执行技能包内脚本。Codex 使用 ephemeral/read-only/ignore-user-config，关闭 shell/unified-exec/apps/hooks/multi-agent/remote-plugin/web search 与项目指令读取；保留 execpolicy 规则，不使用 ignore-rules。Claude 使用 safe-mode/restricted、空 tools、严格空 MCP 和无会话持久化。临时目录退出后清理；Codex 只读最终消息文件，Claude 只接受成功 JSON result。过旧 CLI 不识别约束参数时明确失败，不回退成更宽权限的命令。

Skills 详情新增翻译页：明确点击才发送，显示 provider/语言/范围、持久历史、旧版本标记、取消及复制译文；首次历史加载完成前不可发起，避免隐藏已有活动请求。切换技能/版本卸载旧视图，延迟返回不能串入另一技能；旧结果可回看但不覆盖当前来源。实际四 provider 的账号、模型可用性和翻译质量需分别做 M3-04 验收，配置存在与 fixture 通过均不能代替真实调用。

## M3 历史、交互与当前验收边界

翻译历史按 skillId 扫描 request 上下文，仅按关联 operationId 读取操作并核验 action/source 身份；返回最近 20 条终态与全部运行中任务。共用操作列表返回最近 1000 条终态与全部运行中任务，限制作用于返回集合，不拒绝更大的持久库，也不删除旧记录。owner 锁释放后重读终态再判断 interrupted。旧结果仍可按 operationId 查询。当前平面目录仍需顺序扫描，返回量有界不等于扫描成本恒定。

Skills 命令面板支持 Cmd/Ctrl+K、查询和键盘选择，只触发本地导航或显式复制；摘要翻译命令打开面板，不发送请求。分享文本含原文、可预览相关文件和未预览资源说明，按最长反引号生成完整围栏；不上传或发布。剪贴板失败显示只读可选文本，供用户复制。

四 provider 的执行 fixture 与原生合成进程/重启已验证；真实账号调用按用户选择保持 blocked。G1 视觉/公开 HTTPS 来源与普通原生验收通过；禁止源码读取的桌面验收因 sandbox-exec 窗口不可观察仍 blocked，不以随包 CLI 隔离成功替代。


## 2026-10-25 功能归属与退役范围

用户明确只迁移管理功能并清理源 Manager；M4-01 模块、UI/CLI/IPC 与专属测试已撤下，后续旧数据备份/暂存/恢复不再实施。源 apps/ 与专属启动/打包入口移除，内容检查独立运行。真实 provider 与 G1 桌面隔离仍为历史验证限制，未因范围调整宣称通过。安装服务保留旧清单只读阻止和 Loom v2 事务恢复。
