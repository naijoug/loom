# Research R4：网站研究

2026-11-06。**实施中，不是整体验收。** R0–R4 后端阶段通过；R5–R7 与 E0–E2 未开始。入口为 [实施计划](../plans/2026-10-28/04:41-research-targets-and-experiments.md)。`LOOM_RESEARCH_TARGETS` 仍默认关闭，目标 UI 未接入。

## 交付

- 新增 `browser_host`。每次采集使用新的 user-data-dir、`--remote-debugging-port=0` 和 `--remote-debugging-address=127.0.0.1`。调试端口只从本次 profile 的 `DevToolsActivePort` 读取，并确认监听套接字属于这次进程树。关闭只回收这个进程组。
- 网站预检接受公开 `http(s)` URL，写入解析后的浏览器路径，并明确写出“会加载并执行页面脚本”。拒绝 `file:`、`javascript:`、凭据和私网字面地址。生产构建也拒绝 loopback；测试构建允许 `127.0.0.1`，以便受控站点夹具运行。
- 采集调用 REA `list_browser_targets` / `inspect_web_page`，只接受与本次页面 ID 和 origin 一致的结果。脚本文本发布为报告快照。一张 PNG 截图只写入原始证据，模型视图只保留字节数和“未送入文本提示”。
- 模型视图继续去掉 cookie、Authorization 和 URL 查询串。

## 能强制和不能强制

能强制：

- 不使用用户默认浏览器 profile，也不向其他进程组发信号。夹具浏览器关闭后，旁边的 `sleep` 仍在。
- 调试端口不是 loopback，或不属于本次进程时，返回 `browser_port_not_loopback` / `browser_port_not_owned`，不把该端口交给 REA。
- 页面最终 origin 不同返回 `cross_origin_redirect` 或 `cross_origin_navigation`。浏览器在端口就绪前退出返回 `browser_exited_before_ready:<code>`。取消返回 `cancelled`。
- 文本提示里看不到 cookie / token；脚本标记 `site-marker` 仍在，截图文件以 PNG 魔数开头。

不能强制，需要按这个边界理解：

- origin 检查发生在 Chrome 已经跟随重定向之后，不能预先拦住这次导航。
- 没有网络出口沙箱。页面脚本仍可能请求 CDN 或其他主机；子资源正文不会原样进入模型视图，但请求本身不会被 Loom 拦截。
- 只拒绝字面私网地址和 localhost 名称。公开域名若解析到私网，本阶段不做 DNS 级拦截。R0 也没有证明浏览器网络被沙箱化。
- `--remote-allow-origins=*` 是因为端口由 Chrome 自己选择后才知道；套接字仍限制在 loopback。这不等于允许附着任意 CDP 端点。
- Linux 用 `/proc/net/tcp` 核对套接字。macOS 代码改为调用 `lsof` / `ps`，本次 Linux 环境没有跑 macOS 路径。
- 本环境的默认测试用 CDP 兼容假浏览器和符合 6.8.0 schema 的假 REA 跑通研究链路。另外一次显式 ignored 测试用本机 Google Chrome 156.0.8078.47 打开本机页面、保存真实 PNG，并解释跨端口重定向。没有安装 npm 上的 `rea-agents`，因此这次没有把真实 REA 再接到 Chrome；R0 已在 macOS 上用真实 REA 6.8.0 观察过受控网站。

## 验证

Rust 1.90.0：

- `cargo test --manifest-path src-tauri/Cargo.toml --lib`：**391 passed / 0 failed / 36 ignored**。
- 忽略项 `real_chrome_observes_a_local_page_and_explains_a_redirect` 在本机单独运行通过，约 8.6 秒。它不计入上面的 391。
- `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --all-features -- -D warnings` 通过。契约样本 `researchWebsiteRun` 的 Rust 与前端对照测试通过。
- 未跑真实模型、原生桌面、`pnpm tauri build`，也没有打开目标 UI。
