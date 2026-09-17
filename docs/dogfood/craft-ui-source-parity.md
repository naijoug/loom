# Craft UI source parity (loom ← craft-agents-oss)

Source of truth: [`craft-ai-agents/craft-agents-oss`](https://github.com/craft-ai-agents/craft-agents-oss) (not screenshots).

## Mirrored this pass

| Craft | loom |
| --- | --- |
| `apps/electron/.../app-shell/SessionSearchHeader.tsx` | `ChatInbox` search chrome (muted 32px field, clear) |
| `SessionItem` / entity row (13px title, subtitle, trailing relative time, selection bar) | `ChatInbox` row layout + CSS |
| `ChatDisplay` centered column + floating composer card | `ChatTranscript` inner max-width + `ChatComposer` shell |
| `UserMessageBubble` right-aligned bubble | `.chat-user-bubble` |
| `TurnCard` + activity rows | `.chat-turn` + `.chat-activity` collapsible tool rows |

## Intentionally not ported yet

- Full `AppShell` panel stack / multi-panel / kanban
- Compact mode session menus, multi-select, labels/status taxonomy
- Exact Tailwind token theme / motion library
- Electron-only stoplight / focus zones
- MCP / Sources / Skills side panels
- Five-state Craft inbox taxonomy (loom keeps active / needs_attention / archived)

## Backend boundary

Keep Tauri + ProcessSupervisor chat IPC. Do not adopt Craft Electron session backend.
