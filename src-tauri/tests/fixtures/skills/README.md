# Skills 兼容样本

这些是依据源 `skills@a245966` 的 API、Rust 与 skills-linker 序列化代码构造的合成数据，不来自用户缓存。

- `legacy-library.json`：Web GitHub API camelCase 字段及桌面/GitLab snake_case 字段。
- `legacy-install-manifest.json`：copy、symlink、旧 cache 与旧 bundle 来源路径；均缺少新版本的 digest/使用者归属，必须标为未核实。
- `legacy-install-manifest.tsv`：旧 TSV 的名称、模式、来源、时间四列。

M4 需通过迁移器对这些样本执行规划、接管和恢复测试。仅能解析样本或样本已存在不等于旧数据迁移通过。运行测试时将 `/fixture/` 映射到临时目录，禁止访问这些字面路径或真实用户目录。
