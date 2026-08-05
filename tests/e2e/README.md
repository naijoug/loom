# 完整版本验收

运行完整的本地回归链路：

```bash
pnpm e2e:complete
```

默认执行静态检查、前后端测试、流程冒烟、交互冒烟和深浅主题视觉冒烟。需要同时生成 Tauri 发布包时运行：

```bash
LOOM_E2E_RELEASE=1 pnpm e2e:complete
```

视觉产物默认保存在 `/tmp/loom-visual-smoke`；脚本结束时会停止本仓库的预览进程。
