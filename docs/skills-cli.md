# Skills 命令行

状态：无窗口 CLI、TUI、ng 已接入并完成 M2c 当前范围验收；完整迁移仍等待 GitHub API、翻译、旧数据接管与 G2。当前 CLI 与 Settings → Skills 共用 Rust 来源、安装、共享指令、诊断和恢复服务，不启动 Tauri 窗口。

## 入口

开发环境：

```bash
cargo build --manifest-path src-tauri/Cargo.toml --bin loom-skills
tools/skills-cli/skills-linker --help
```

兼容启动器依次查找 `LOOM_SKILLS_CLI` 指定路径、本地/随包位置、仓库 release/debug 产物及 PATH；不自动构建或安装依赖。需要固定到某一构建时，将 `LOOM_SKILLS_CLI` 设为该可执行文件的绝对路径。

应用包内无需配置 PATH：

```bash
"/Applications/Loom.app/Contents/MacOS/loom-skills" list --json
```

Tauri 同时打包 `loom` 和 `loom-skills`；`default-run = "loom"` 保持桌面主入口。实际移动 `.app` 到含空格路径、从 checkout 外目录运行 CLI 的验证见 [实施证据](dogfood/2026-10-21-skills-migration.md)。

## 来源与目标

默认读取 Loom 来源库；`--skills-root DIR` 显式选择本地来源。list/detail/status/doctor 不因显式来源而添加库登记，install 会登记来源，以便桌面识别同一安装。默认作用域为 global、工具为 Codex、安装方式为 symlink；桌面安装默认 copy。

```bash
tools/skills-cli/skills-linker list --skills-root /path/to/skills --category manual --json
tools/skills-cli/skills-linker install --skills-root /path/to/skills \
  --scope project --project-root /path/to/project --tool codex --mode copy skill-id
```

`chatgpt` 归一到 Codex，`claude` 归一到 Claude Code。可重复 `--tool` 或用逗号选择多个工具；多个目标共用同一指令块时，选择全部使用者才能改变共享内容：

```bash
tools/skills-cli/skills-linker install --scope project --project-root /path/to/project \
  --tool codex,amp --mode copy --force skill-id
```

CLI 的项目路径是用户通过参数或当前工作目录作出的显式授权；桌面仍要求项目已登记。`--target-dir DIR` 为 CLI 授权独立的自定义 skills 目录，仅支持一个工具，不能是文件系统根、HOME 或项目根。已知工具仍使用其 scope 对应的指令/命令文件；`--tool custom` 没有推断出的指令文件或 wrapper，只有技能包。自定义目标恢复时须重新提供同一目标授权。

## 安装、更新与恢复

- 不传技能名的 install 选择来源/分类内全部技能。短名冲突时拒绝选择，使用 list 返回的 `source_skill_id` 区分来源。
- `--preview` 保存来源登记、操作记录与可审查规划，但不提交安装文件；可配合 `--json` 查看实际目标和指令变更。
- 已有 copy 默认跳过；`--force` 只允许显式替换受管安装并保留备份。已验证且仍指向同一来源的 symlink 重装会同步指令；远程固定快照切换仍需显式替换。
- `--with-slash-commands` 为 manual 技能生成 Codex/Claude wrapper。auto/global 的 inject.md 自动纳入安装规划；cron 不注入。
- status 和 doctor 共用清单与只读诊断；doctor 的 current/not-applicable 为成功，missing/stale/malformed 为失败，不自动修复文件。
- uninstall 可凭保存的安装名、来源身份或 installationId 工作，来源移除后仍可卸载。显式请求已不存在的普通安装名返回 missing；无受管归属的现存内容和用户修改过的安装即使加 `--force` 也不删除。

```bash
tools/skills-cli/skills-linker status --scope project --project-root /path/to/project --json
tools/skills-cli/skills-linker doctor --scope project --project-root /path/to/project --json
tools/skills-cli/skills-linker uninstall --scope project --project-root /path/to/project skill-id
tools/skills-cli/skills-linker pending --scope project --project-root /path/to/project --json
tools/skills-cli/skills-linker recover --scope project --project-root /path/to/project operation-id
```

无技能名的 uninstall 选择所选目标内的受管安装；提供 `--skills-root` 时进一步限定来源，提供 `--category` 时按安装包元数据/路径筛选。旧 v1/TSV 与未知归属继续只读保护，等待 M4 显式接管。

## 输出与协调

兼容保留 list 的 skills_root/skills/items/count（item 的 id/path/category/title/summary/manual_group）及操作的 action/results（skill/status/reason）。扩展字段包含来源稳定身份、目标、诊断和规划；来源定位不再依赖相邻 skills checkout。

成功退出 0，错误或部分失败退出 1，无命令参数展示帮助并退出 2，显式 `--help` 退出 0。错误写 stderr；JSON 成功结果写 stdout。部分失败返回非零是对旧脚本“有 error 项仍退出 0”的修正，不能将部分成功当全部成功。

库目录解析顺序：`--data-dir` → `LOOM_SKILLS_DATA_DIR` → `LOOM_PROFILE_DIR/skills` → 默认 app data/skills。库覆盖不改变同一用户的固定协调目录。CLI 持生命周期 lease 到输出结束，应用更新独占准入期间拒绝新 CLI 操作；提交与恢复沿用桌面的 writer、指纹和 pending 协议。测试注入独立路径，不替换 HOME/CODEX_HOME。

旧 Manager 尚未退役；历史管理器不能同时写 Loom v2 目标。旧源码和转发入口在 G2 通过后的 M7 统一移除。

## TUI 与 ng

```bash
tools/skills-cli/tui --skills-root /path/to/skills --scope project --project-root /path/to/project
tools/skills-cli/ng --skills-root /path/to/skills list
tools/skills-cli/ng --skills-root /path/to/skills plan
tools/skills-cli/ng --skills-root /path/to/skills plan review "payment module rewrite"
```

TUI 支持切换动作、目标、copy/symlink 与替换策略、manual wrapper、选择/清空/全选技能和重复操作。`preview selected skills` 显示包正文；`execute now` 先展示实际目标和托管文件，只有选择 `Apply reviewed changes` 才应用这些已保存的规划，默认 `Back` 可取消。源文件、目标或归属在预览后变化会冲突；多个规划共享文件时，仅沿用本批次已验证提交的后状态，不重新生成另一份计划。

`ng` 无参数时选择 manual 技能并输出触发建议；`ng list` 按原分组顺序展示，`ng plan` 按激活类别说明用途；`ng <group> <verb> [extra...]` 或直接技能 ID 生成建议。它只输出文本，不执行 Agent、不自动发送提示词。额外文本即使含 shell 语法也按字面处理；以 `-` 开头的文本可放在 `--` 后。

随包入口（无需 checkout 或 PATH 配置）：

```bash
"/Applications/Loom.app/Contents/Resources/tools/skills-cli/ng" list
"/Applications/Loom.app/Contents/Resources/tools/skills-cli/tui" --scope project --project-root /path/to/project
```

TUI 需要本机 fzf；缺失时给出错误，`ng` 无参数则回退为 manual 列表。只有显式 `--auto-install-fzf` 且 fzf 缺失时才调用 `brew install fzf`，不自动安装 Homebrew；有 10 分钟超时、取消和进程组清理，执行记录可从 Skills 操作历史读取。本次依赖安装分支用受控替身验收，没有安装真实软件。

fzf 只返回经过校验的数字条目身份。技能文本不进入 shell 预览命令；显示时移除终端控制字符。为保证选择行为，子进程不继承 FZF_DEFAULT_OPTS、FZF_DEFAULT_OPTS_FILE、FZF_DEFAULT_COMMAND。手动预览由 Rust 读取技能文件。

可选环境验收（默认门禁不要求本机 fzf 或触碰实际用户协调目录）：

```bash
cargo test --manifest-path src-tauri/Cargo.toml actual_fzf_matches_visible_text -- --ignored
LOOM_SKILLS_ACCEPTANCE_BINARY=/absolute/path/to/loom-skills \
  cargo test --manifest-path src-tauri/Cargo.toml actual_cli_recovers_a_fixture -- --ignored --nocapture
```

第二项只写自行创建的临时项目和事务，用实际用户协调域验证另一 dataDir 的真实 CLI 恢复，并清理自身 pending 标记。
