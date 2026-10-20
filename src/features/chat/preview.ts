import type { AppState } from "../../state/model";
import { initialAppState } from "../../state/model";
import { mockChatStore } from "./mockStore";
/** Synthetic browser evidence only; main enables this solely in Vite dev mode. */
export function createPreviewState(): AppState {
  const projects = ["loom", "notes-site"].map(name => ({ id: name, name, path: `/preview/${name}`, detectedStacks: ["React", "Rust"], suggestedCommands: [], isGitRepository: true, hasUncommittedChanges: false, loomDirReady: true, schemaVersion: 1 }));
  const agents: AppState["agents"] = [
    { id: "agent-claude", name: "Claude Code", command: "claude", adapterType: "claude_code_cli", available: true },
    { id: "agent-codex", name: "Codex", command: "codex", adapterType: "codex_cli", available: true },
    { id: "agent-opencode", name: "OpenCode", command: "opencode", adapterType: "opencode_cli", available: false },
  ].map(agent => ({ ...agent, adapterType: agent.adapterType as "claude_code_cli" | "codex_cli" | "opencode_cli", enabled: true, args: [], capabilities: [], workingDirectoryPolicy: "project_root", canWriteFiles: true, canRunCommands: true }));
  const chat = mockChatStore.create({ projectPath: projects[0].path, agentId: "agent-claude", permissionMode: "ask", title: "Refactor the chat entry" });
  chat.messages = [
    { id: "user-preview", role: "user", content: "Explore how sessions are stored, then simplify the chat interface.", status: "complete", createdAtMs: 1 },
    { id: "assistant-preview", role: "assistant", content: "Each project keeps its own conversation history in `.loom/chat/v2`. The interface can group those sessions without moving their data.\n\n- Keep the existing journal and resume handles.\n- Put Research and Code in the session header.\n- Select the local Agent from the composer.", status: "complete", createdAtMs: 2, parts: [
      { type: "tool", callId: "read-1", name: "Read", kind: "read", target: "src-tauri/src/chat/repository.rs", status: "done", outputSummary: "The repository stores snapshots and an append-only event journal." },
      { type: "tool", callId: "edit-1", name: "Edit", kind: "edit", target: "src/App.tsx", status: "done", diffStats: { added: 3, removed: 8 }, diffPreview: "-<WorkflowShell />\n+<ChatShell>\n+  <ChatPage />\n+</ChatShell>" },
      { type: "tool", callId: "bash-1", name: "Bash", kind: "bash", target: "pnpm test", status: "done", exitCode: 0, outputSummary: "Synthetic preview output. This is not real validation evidence." },
    ] },
  ];
  mockChatStore.create({ projectPath: projects[0].path, agentId: "agent-codex", title: "Explore native resume", permissionMode: "explore" });
  mockChatStore.create({ projectPath: projects[1].path, agentId: "agent-codex", title: "Review the publishing flow", permissionMode: "explore" });
  return { ...initialAppState, projects: { current: projects[0], recent: projects }, agents, app: { ...initialAppState.app, activeProjectId: projects[0].id } };
}
