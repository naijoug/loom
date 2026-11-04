# Loom 安全策略

当前运行产品是本机 CLI Chat，另有独立 Skills 模块、Learn 学习研究模块与 Research 调研模块。Rust 校验本地文件、项目路径与进程边界；React 的控件禁用和确认对话框不构成安全沙箱。

## Skills 写入边界（安装接线已实现，完整迁移实施中）

- 当前基础模块只接收显式路径，不从模型输出或前端接受可直接执行的命令。安装目标必须通过后端已登记项目/当前用户目录解析；自定义 CLI 目标须显式指定和校验。
- 用户级协调域与来源库 dataDir 分离。生命周期共享/独占 lease 先于 writer 锁；OS 锁负责存活期互斥，持久化 pending 索引负责崩溃后恢复判断，不能互相替代。
- 写前先同步准备日志及恢复索引；损坏或未来 schema 不视为空。重叠 write set 有未完成事务时阻止新写入；恢复日志不存在也不能清除标记。
- 共享指令按实际文件/marker/使用者集合管理；未核实旧归属和用户修改不得被卸载清除。远程快照只在显式更新安装时切换。
- 纯文件操作也须纳入应用更新准入；独立 CLI 通过跨进程 lease 协调。安装/恢复与 updater 已接线并有服务级回归；共享指令 ledger 与事务已接线；无窗口 CLI 核心已接同一固定协调域，并持 lease 到输出结束；TUI/ng 也持有同一生命周期 lease；只把受校验的选择交给 Rust 服务。
- 元数据文件 `0600`、协调目录 `0700`（Unix），读取有界，拒绝 symlink 元数据文件。目录/文件检查不构成对任意恶意本机进程的 OS 沙箱。

## Skills 来源网络与诊断

- GitHub API 固定 HTTPS 主机与 GET 路径，禁用重定向；认证仅在 Rust 后端处理，不向前端/操作日志返回 header、body 或 token。错误保留分类，不回显上游正文。
- API 完整包经路径、模式、大小、blob 身份校验后发布固定 commit 快照；不运行包内脚本，不安装缺文件或截断树。
- Git 保留用户 SSH 配置及身份通路；仅未配置时补非交互 BatchMode，不禁用主机密钥校验。Git/SSH stderr 持久记录使用受控错误分类，原始 wrapper 输出不进入日志。

- Skills 翻译配置使用私有版本化文件，前端仅获得密钥存在性和来源状态；显式 keep/set/clear 与 expectedRevision 防止误清除/旧状态覆盖。配置操作纳入生命周期和 writer/pending 准入，不因保存设置自动调用模型。

## Skills 翻译执行

- 翻译仅由显式点击启动，固定技能版本、provider 与输入摘要；源正文不修改，不复用 Chat 会话。远程 endpoint 固定 HTTPS、拒绝重定向，不自动重试；超时、取消与输出大小受控。
- 本地翻译使用独立私有临时目录、只读/禁用工具的 CLI 参数和进程组监督，stdin 传输正文；结果按 operationId 保存，日志仅记录脱敏元数据。实际工具隔离保障仍需对应 CLI 真实版本验收，不能用提示词代替边界。
- 分享动作仅复制文本；剪贴板失败显示可选文本，不上传 Gist。网页合成 Skills API 仅由 main 的显式 DEV fixture 分支注入，生产默认不使用 mock。

Agent 层与明确的非 Agent 进程例外见 [Agent 层契约](architecture/agent-layer.md)。Chat/Learn/Research/Discuss 的生产 Agent 生命周期只通过公共接口；退出信号实现仍由同一 supervisor 管理。

## CLI 执行

- `chat_send` 只访问已登记的 canonical 项目；存储会话路径必须匹配项目，准备后的 cwd 必须位于该项目内。
- 后端校验 Agent 启用/可用状态，以及可写和运行命令的配置权限。运行中拒绝切换 Agent、权限、归档或清除续聊。
- Read-only 显式映射 CLI 只读/plan 策略；Ask 使用可写策略，Auto 使用适配器声明的自动策略。实际保障按每种 CLI 的真实测试判断。
- Loom 不拦截 CLI 内部逐次工具调用。Code 的确认仅为每回合交互授权，没有独立的后端审批令牌；不能视为逐工具审批。
- 自定义 CLI 不接受 shell wrapper，只按参数数组启动专用程序。Loom 不为自定义程序额外提供文件沙箱。
- 原生续聊只接受有界 ID，绑定 Agent、程序、参数、项目和权限指纹。不能把存储的命令字符串当 shell 执行；配置改变后清除句柄并重放有界历史。
- 子进程使用独立进程组。停止/超时先 TERM，再升级 KILL；正常退出等待运行终止与证据保存。历史 PID 只用于诊断，不用于重启后盲杀进程。

## 模型来源与 API Key

- provider 保存在配置目录的私有 `providers.json`（与 `agents.json` 同目录；目录 0700、文件 0600、原子写）。写入持排他锁并校验 `expectedRevision`，桌面与 `loom-agent` 不会交错覆盖；被 Agent 引用的 provider 不能删除。
- key 来源按优先级：`keyEnv` 指定的环境变量（运行时读取）→ 已保存的 key → 本机/内网网关可无 key。IPC、CLI 列表和诊断只给出“是否有 key、来自哪里”，从不返回 key 或其片段；前端 key 输入框不回填，保存后从状态中丢弃，编辑默认 keep，清除需确认。
- Base URL：只接受 https；明文 http 仅限 loopback、私有网段、链路本地、`localhost` / `*.localhost` / `*.local`。拒绝 URL 中的用户名密码、query、fragment，长度 ≤2048。请求拒绝重定向（key 不会被转发到其它主机），本机/内网地址不走 shell 代理，连接超时 10 秒。
- 上游错误按 HTTP 状态分类（认证失败、未找到、限流等），不读取、不回显响应正文；只保留短标识符形式的错误码。`provider test` 只列模型，不消耗 token。
- 模式 B 注入只作用于单次调用：key 只进入子进程环境，不进 argv、执行记录和日志；清除继承的同族变量（`ANTHROPIC_*`、`OPENAI_*` 等），不写 CLI 的全局配置，不读取 CLI 的订阅登录凭据。各 CLI 的变量见 [适配器](agent-adapter.md#模型来源注入模式-b)。
- `loom-agent` 没有 `--key` 参数；key 只从管道或文件形式的 stdin 读入（≤16 KiB），stdin 是终端时拒绝。执行记录不存提示词正文、进程环境和 key。
- 直连 API（模式 C）没有工具与文件访问；需要工具的功能拒绝 `api` Agent。这依赖后端 `needs_tools` 校验，不依赖提示词。

## Learn

- 只接受 `https://github.com/<owner>/<repo>` 公开仓库与受校验的分支 / tag / 完整 commit；不支持 SSH、私有仓库或其他主机。仓库内容一律视为不可信输入。
- 拉取复用 `git_fetch.rs` 加固配置（禁用 hooks、fsmonitor、ext 传输、子模块递归和 LFS/smudge 过滤器，`.git/info/attributes` 关闭过滤属性）；先 `--no-checkout` 浅克隆，`ls-tree` 校验文件数与体积上限后才检出。Git stderr 只以受控分类持久化。
- 研究数据只写 `<appData>/learn/studies/<studyId>/`；研究 ID 为受限单段标识，研究目录与源码读取路径拒绝 symlink 和逃逸。Loom 不运行被研究项目的任何代码。
- Agent 只以 `explore`（只读 / plan）权限、cwd = checkout 运行，不接受自定义 CLI；每次运行前后比对 checkout 指纹，变化即判作业失败。这是检测而非 OS 沙箱：实际只读保障取决于各 CLI 的策略实现。
- 模型输出只按 schema 与引用校验后使用；无效引用以警告样式显示，不静默信任。Markdown 渲染沿用下文的安全规则，`loom-src:` / `loom-wiki:` 仅在 Learn 页面内转为应用内操作。
- 作业日志沿用 CLI 执行的输出限额和脱敏；`learn_*` 命令仅接受主窗口调用。
- 新建页的 GitHub 热门与发现对话只由 Rust 访问固定端点 `https://api.github.com/search/repositories`：不跟随重定向，10 s 连接 / 20 s 总超时，响应 ≤ 4 MiB，错误只给受控文案、不回显响应正文；可选读取后端环境变量 `GITHUB_TOKEN` / `GH_TOKEN`（敏感头，不写日志）。Agent 给出的仓库名按 `owner/repo` 字符集校验、搜索词去控制字符并限长，每回合至多 3 次搜索；卡片 URL 由 Loom 按 `https://github.com/<owner>/<name>` 生成，不使用响应中的链接。
- 发现对话的 Agent 以 `explore`、`needs_tools: false` 运行，cwd 为 `<appData>/learn/discover/workspace/` 空目录；对话历史在提示词中作为非可信资料隔离。执行记录写入 `<appData>/learn/discover/runs/<runId>/`（脱敏 prompt / stdout / stderr / run.json，保留最近 50 个）。

## Research

- 研究数据只写 `<appData>/research/<researchId>/`；研究 ID 为受限单段标识。未选项目时 cwd 是该目录下的空 `workspace/`。
- 只能选择已在 Loom 打开的项目（规范路径匹配项目列表）；开始、追问、重试、校验报告引用与读取源码前都重新确认，项目移出列表后不再读取它。
- Agent 只以 `explore`（只读 / plan）权限运行，不接受自定义 CLI。选了项目时运行前后比对 `git status --porcelain` 与 HEAD，变化只记警告：用户可能同时在改项目，且这是检测而非 OS 沙箱，实际只读保障取决于各 CLI 的策略实现。Loom 不运行项目代码。
- 报告引用经 `source_files.rs` 校验（拒绝逃逸与 symlink），无效引用以警告样式显示；外部链接只接受 `http(s)` 并在系统浏览器打开。
- 作业日志沿用 CLI 执行的输出限额和脱敏；`research_*` 命令仅接受主窗口调用。

## Discuss

- 只允许 main 窗口调用，选择已登记项目；创建/重试/源码读取重新验证项目归属。任务始终经 Agent 层以 `explore` 执行，最多 4 并发、每次 15 分钟，沿用 runner 的输出限额、日志脱敏和进程组停止。
- UI 在开始前确认 `2N+1` 次调用，单项重试确认 1 次；后端验证计数与参与者，确认字段不是不可伪造的授权凭证，也不是逐工具审批。失败项不会导致整批自动重试。
- 议题与同伴回答是非可信资料，提示词明确隔离；不把它们当作权限控制。共享 Markdown 不执行 HTML。引用读取通过 `source_files`，不允许路径逃逸或 symlink。
- 讨论/作业 ID 与内部参与者路径键受控；仅写 `<appData>/discuss/`。任务重试保留旧尝试日志，后续结果标为过时。无活动 lease 的未完成任务标中断，不自动调用模型。详见 [Discuss 契约](architecture/discuss-contracts.md)。

## 存储和输出

- `.loom/chat/v2` 使用完整换行 journal 作为权威记录，项目 writer 锁避免多进程竞争。缺失、损坏或未来 schema 明确报错；旧 Task 数据不加载、不修改、不删除。
- stdout/stderr 并发排流、限制字节和行数，单回合保留脱敏的程序、argv、cwd、时间、状态、退出码与终止原因。
- JSON 协议按字段脱敏，避免误删 usage 计数；纯文本中的凭据赋值和 Bearer 信息替换。不会记录完整进程环境。
- 会话/回合 ID 为受限单段标识；读取日志时从已关联回合派生路径，拒绝 symlink。诊断包只包含有界 Chat 日志，不读取旧 Task 文件。
- 会话导出在项目内部生成独立目录；诊断导出由保存对话框选择目标。导出内容仍可能包含用户输入和代码，分享前须检查；脱敏不是完整的秘密检测。
- 工具输出与 diff 使用转义后的文本节点。Markdown 由 `react-markdown` 按 GFM 生成 React 元素，不使用 `dangerouslySetInnerHTML`；原始 HTML 节点在解析阶段改为字面文本，不解析、不执行。Tauri CSP 不允许任意远程脚本，远程图片不加载，改为链接。
- 链接只有 `http(s)://` 与 `mailto:` 可点击，点击时阻止 WebView 导航，经 opener 插件交给系统默认浏览器；能力文件的 `opener:allow-open-url` 作用域限定为这三类 URL。`javascript:`、`file:`、相对路径等显示为普通文本。

Loom 本身不是操作系统级沙箱。CLI 使用当前用户身份及自身配置，可能访问远端模型。只应配置可信可执行文件；外部账户、全局 CLI 配置和磁盘二进制替换不在续聊配置指纹覆盖范围内。

具体协议见 [Chat 契约](architecture/chat-contracts.md)，CLI 证据见 [适配器](agent-adapter.md) 和 [本轮验收](dogfood/2026-10-20-simple-chat-acceptance.md)。

## 旧 Skills 数据边界

用户已取消旧 Manager 数据迁移；不提供发现、迁移预览、备份/导入/接管或回滚旧数据的入口。旧 v1/TSV、损坏清单和未知归属仍拒绝覆盖或删除。已有 Loom v2 安装的事务备份/恢复继续保留，不等于旧数据接管。源代码退役不授权删除私人 app data、缓存、凭据或已安装技能。
