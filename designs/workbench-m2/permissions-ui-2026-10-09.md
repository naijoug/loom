# 权限设置原生检查

2026-10-09，Loom debug bundle，1280×820，独立 `.tmp/permissions-native-data`、fixture 模式。

- CUA 读取/操作真实 Tauri 窗口，观察权限范围、JSON 编辑区、校验错误、版本与保存/重置控件。
- 初始弹窗被共享 `max-width:480px` 限制；改为最大 90vw、预期宽度 760px，原生截图确认内容和底部控件完整可见。
- 保存、正常退出、重启后读取、重置均有原生 AX 状态及 catalog 事件相互印证。
- 截图当场检查，未另存像素文件；这里记录观察，不冒充截图或录屏。
- 当前页面是功能接线用 JSON 编辑器。Craft 的规则表格与 EditPopover、完整设置导航/视觉对齐仍待 M3/M4。

行为和数据证据见 [权限持久化记录](../../docs/dogfood/craft-v014/m2-permissions-store-progress.md)。
