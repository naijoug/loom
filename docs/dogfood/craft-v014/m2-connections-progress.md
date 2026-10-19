# M2 连接目录与 Keychain 切片

日期：2026-10-07。基线：`4d0574f` 加本轮修改。当前完成连接目录、发送快照、Rust 凭据边界和前端配置流程；**原生界面验收因 Mac 锁屏待补，真实模型连接未验证**。

## 已实现

- 全局命名连接，Claude/Pi/PiCompat 配置、API key/bearer/无认证、端点/模型列表/默认模型和行为提示；revision 冲突检查与 operationId 重试。
- 发送时冻结完整连接快照，运行绑定校验 digest 与默认/显式模型。首次接收消息后锁定连接身份；导入历史不继承连接。
- macOS 系统 Keychain 保存秘密，catalog 仅保存不透明引用。秘密输入不实现 Serialize/Debug，宿主缓冲使用 zeroize；专门 Tauri 保存命令限制可信主窗口。
- 改变凭据目的端时要求重新输入。每次更换秘密使用新 slot；旧 slot 保留给已接受配置，不将新密钥发送到旧快照目的端。不确定提交保留新 slot，重试不会重复创建连接。
- 配置表单、连接/模型选择器、空凭据保留、成功后清空输入与明确“未验证”状态。没有伪造测试连接成功或开放真实模型发送。

## 验证证据

| 检查 | 当前结果 |
| --- | --- |
| `pnpm check:core` | 68 个测试入口通过：原 60 + 8 连接测试；fmt、Clippy、类型漂移与协议 TS 编译通过。 |
| 连接行为 | URL/凭据嵌入拒绝、revision 冲突、幂等保存、重启、发送配置冻结、运行模型/digest 不匹配拒绝、身份锁定和旧 journal 可选字段兼容均通过。 |
| Tauri Workbench | 11 项常规测试通过，1 项显式 Keychain 测试默认忽略；严格 Clippy 通过。新增 5 项覆盖秘密不落 catalog/响应、vault 失败、旧修订拒绝、不确定提交恢复和目的端变更重入。 |
| 真实 macOS Keychain | 显式运行 `native_keychain_roundtrip_and_catalog_contains_no_secret -- --ignored`，1 项通过；只创建新临时 namespace 的合成密钥，验证读取、catalog 无秘密、删除后不存在。测试清理完成。 |
| 前端与契约 | 17 项定向测试通过；保存重试复用 operationId、成功后清空秘密、编辑保留、专用命令和响应归属，以及原会话/补流行为。 |
| Rust 契约与调试清理 | Rust contracts 6 项通过；调试脚本 3 项通过。发现普通沙箱无法运行 ps 时 stop 会误报成功，已改为失败并保留 PID 提示；随后在具备权限的环境清理，原运行句柄确认退出。 |
| 构建 | `pnpm build` 与 macOS debug app bundle 通过；新增依赖为标准 URL 解析、系统 Keychain 与零化缓冲，Tauri lockfile 保留了不相关依赖版本。 |
| 原生界面 | 未通过：启动隔离 fixture 后，CUA 返回 Mac locked，无法获取/操作界面。已请求手动解锁；未绕过锁屏或用 DOM 单测冒充原生验收。调试实例已 stop。 |

机器摘要与源码/app hash 见 [记录](m2-connections-evidence.json)。本轮没有调用外部模型，没有读取用户已有 Keychain 条目，没有向模型端点发送凭据。

## 待补原生步骤

使用 `.tmp/workbench-connections-2026-10-07/data`（仅创建了“连接凭据验收”工作区和空会话）与 `scripts/debug.sh` 重新启动。检查配置表单布局，保存合成 API key 连接，确认字段不回显，重启后空字段保留凭据/重命名，选择连接后验证发送快照。实际调用仍必须明确为 fixture；测试条目清理由精确 namespace/account 限定，不能删除用户凭据。

## M2 剩余工作

生产 worker/SDK 宿主、模型连接测试和真实账户验收；OAuth、IAM、服务账号与环境认证；完整自定义端点传输/query 配置；动态模型能力与 provider 目录；连接默认项/删除与旧凭据回收；Windows/Linux vault；完整权限策略、resume/compact/steer、usage 和历史上下文。中途行为/图像选项当前仅持久化，执行效果尚未完成。

M0–M8 的整体目标保持不变，本切片不作为 M2 或最终交付完成证明。
