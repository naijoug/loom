# Craft Agents attribution

Source: https://github.com/craft-ai-agents/craft-agents-oss/tree/73bd9c2a3573158bea880984eb8d5fdb41e0cac2 (v0.14.0).

`crates/loom-runtime/resources/craft-v014-permissions.json` is an unmodified copy of `apps/electron/resources/permissions/default.json`.
The Rust shell policy in `crates/loom-runtime/src/shell/` and `shell.rs` adapts the behavior and argument inspection rules from `packages/shared/src/agent/bash-validator.ts` and `mode-manager.ts`. Rust uses Tree-sitter instead of the upstream Bash parser, bounded regular expression evaluation, and explicit rejection of executable redirect expansions.

`crates/loom-runtime/src/policy.rs` and `policy/` adapt mode, source verb, API endpoint and layered rule behavior from `agent/{mode-manager,permissions-config,mcp-tool-names,source-policy}.ts` and `agent/core/{pre-tool-use,guarded-mode}.ts`. Source regexes are scoped structurally rather than by concatenating a prefix; allowlisted write paths also check resolved symlink destinations.

`policy/remember.rs` adapts selector rules and command sets from `agent/core/permission-remember.ts`. Loom preserves selector case, binds directory selectors to resolved targets, includes API source identity, rejects attached hidden curl/wget target options, and persists hashed selectors with an explicit runtime lifetime.

Copyright (c) 2026 Craft Docs Ltd. Preserve the accompanying Apache-2.0 LICENSE and upstream NOTICE when distributing derived portions. This does not grant use of Craft branding or imply endorsement. Loom's unrelated code is not relicensed by this notice.
