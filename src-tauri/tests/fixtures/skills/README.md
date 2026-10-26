# 不支持清单的保护样本

这些合成 JSON v1/TSV 清单用于验证 Loom 不会隐式接管、覆盖或删除旧安装。没有来自用户缓存的数据。

- `legacy-install-manifest.json`：copy、symlink 和旧来源路径，缺少 Loom v2 的 digest/使用者归属。
- `legacy-install-manifest.tsv`：旧名称、模式、来源、时间四列。

旧数据发现、导入与回滚已按用户要求退出范围；这些样本只验证拒绝写入边界。任何 `/fixture/` 字面路径都不得在测试中作为真实用户来源访问。
