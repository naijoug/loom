import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { FolderPlus, PanelLeftOpen } from "lucide-react";
import type { AgentDiagnostic, ChatPermissionMode, ChatSession, ProjectSummary } from "../../domain";
import { useAppState } from "../../state/AppStateContext";
import { useAgentCatalog } from "../../hooks/useAgentCatalog";
import { useProjectBridge } from "../../hooks/useProjectBridge";
import { hasTauriRuntime } from "../../hooks/runtime";
import { useT } from "../../i18n";
import { chatAbort, chatClearResume, chatCreate, chatSetAgent, chatUpdateMeta } from "../../api/chatClient";
import { AddProjectModal } from "../../components/Sidebar/AddProjectModal";
import { ErrorBoundary } from "../../components/common/ErrorBoundary";
import { UpdateButton, useUpdateDrafts } from "../app-update/UpdateProvider";
import { useChatBridge } from "./useChatBridge";
import { useChatExport } from "./useChatExport";
import { mockChatStore } from "./mockStore";
import { chatSendController } from "./state/ChatSendController";
import { defaultAgentId } from "./AgentPicker";
import { ProjectTree } from "./ProjectTree";
import { ChatSessionHeader } from "./ChatSessionHeader";
import { ChatTranscript } from "./ChatTranscript";
import { ChatComposer } from "./ChatComposer";
import { useInlineCardsEnabled } from "./openui/inlineCardsSetting";
import { ChatExportNotice } from "./ChatExportNotice";
import { EmptySession } from "./EmptySession";
import type { Confirmation } from "./ConfirmBanner";
import { LoomMark } from "./icons";
import { parseSlashCommand } from "./slashCommands";
import { toPermissionMode } from "./workMode";
import "./chat.css";

export function ChatPage() {
  const t = useT();
  const { state, dispatch } = useAppState();
  const { loadAgents, diagnoseAgents } = useAgentCatalog();
  const { loadRecentProjects } = useProjectBridge();
  const useBackend = hasTauriRuntime();
  const project = state.projects.current;
  const projectPath = project?.path ?? null;
  const agents = useMemo(() => state.agents.filter(agent => agent.enabled && agent.adapterType !== "dummy"), [state.agents]);
  const [selection, setSelection] = useState<{ project: string | null; id: string | null }>({ project: projectPath, id: null });
  const sessionId = selection.project === projectPath ? selection.id : null;
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const draftKey = JSON.stringify([projectPath, sessionId ?? "new"]);
  const draft = drafts[draftKey] ?? "";
  const setDraft = (value: string) => setDrafts(previous => ({ ...previous, [draftKey]: value }));
  const inlineCards = useInlineCardsEnabled();
  const insertText = (text: string) => setDrafts(previous => {
    const current = previous[draftKey] ?? "";
    return { ...previous, [draftKey]: current.trim() ? `${current}\n\n${text}` : text };
  });
  useUpdateDrafts("legacy-chat", drafts, saved => setDrafts(previous => ({ ...saved, ...previous })));
  const [newMode, setNewMode] = useState<ChatPermissionMode>("explore");
  const [newAgent, setNewAgent] = useState("");
  const [diagnostics, setDiagnostics] = useState<AgentDiagnostic[]>([]);
  const draftAgent = defaultAgentId(agents.map(agent => diagnostics.some(diagnostic => diagnostic.agentId === agent.id && diagnostic.status === "unavailable") ? { ...agent, available: false } : agent), newAgent || localStorage.getItem("loom-last-agent"));
  const [diagnosticsLoading, setDiagnosticsLoading] = useState(false);
  const [revision, setRevision] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [addProjectOpen, setAddProjectOpen] = useState(false);
  const [confirmation, setConfirmation] = useState<Confirmation | null>(null);
  const [collapsed, setCollapsed] = useState(() => window.innerWidth < 760);
  const pending = useRef(new Set<string>());
  const pendingStops = useRef(new Set<string>());
  const [pendingKeys, setPendingKeys] = useState(new Set<string>());
  const [stopKeys, setStopKeys] = useState(new Set<string>());
  const [creating, setCreating] = useState(false);
  const createLock = useRef(false);
  const firstSend = useRef<{ project: string; id: string } | null>(null);
  const current = useRef({ projectPath, sessionId }); current.current = { projectPath, sessionId };
  const changed = useCallback(() => setRevision(value => value + 1), []);
  const { session, setSession, listenReady } = useChatBridge(projectPath, sessionId, useBackend, setError, changed);
  const currentKey = JSON.stringify([projectPath, sessionId]);
  const running = pendingKeys.has(currentKey) || session?.turnStatus === "streaming";
  const isCurrent = () => current.current.projectPath === projectPath && current.current.sessionId === sessionId;
  const exportState = useChatExport(useBackend ? projectPath : null, useBackend ? sessionId : null);
  const refreshDiagnostics = useCallback(async () => {
    setDiagnosticsLoading(true);
    try { setDiagnostics(await diagnoseAgents()); }
    catch (error) { setError(String(error)); }
    finally { setDiagnosticsLoading(false); }
  }, [diagnoseAgents]);
  useEffect(() => { void loadAgents(); void loadRecentProjects(); void refreshDiagnostics(); }, [loadAgents, loadRecentProjects, refreshDiagnostics]);
  useEffect(() => {
    const media = window.matchMedia("(max-width: 759px)");
    const resize = (event: MediaQueryListEvent) => { if (event.matches) setCollapsed(true); };
    media.addEventListener("change", resize); return () => media.removeEventListener("change", resize);
  }, []);
  useEffect(() => { setError(null); setConfirmation(null); }, [projectPath, sessionId]);
  useEffect(() => { if (running) setConfirmation(null); }, [running]);
  useEffect(() => {
    if (!session?.activeTurnId || !projectPath) return;
    const key = JSON.stringify([projectPath, session.id]);
    if (!pendingStops.current.has(key)) return;
    void chatAbort({ projectPath, sessionId: session.id, turnId: session.activeTurnId })
      .catch(error => { if (current.current.projectPath === projectPath && current.current.sessionId === session.id) setError(String(error)); })
      .finally(() => { pendingStops.current.delete(key); setStopKeys(new Set(pendingStops.current)); });
  }, [session?.activeTurnId, session?.id, projectPath]);

  function select(target: ProjectSummary, id: string | null) {
    if (!id) { setNewMode("explore"); localStorage.removeItem(`loom-auto-confirmed:${target.path}:new`); }
    dispatch({ type: "projects/selected", projectId: target.id });
    setSelection({ project: target.path, id }); setError(null);
    if (window.innerWidth < 760) setCollapsed(true);
  }
  async function createAndSend() {
    if (!projectPath || !draft.trim() || !draftAgent || createLock.current || !listenReady) return;
    createLock.current = true; setCreating(true); setError(null);
    const submitted = draft;
    try {
      const created = useBackend ? await chatCreate({ projectPath, agentId: draftAgent, permissionMode: newMode })
        : mockChatStore.create({ projectPath, agentId: draftAgent, permissionMode: newMode });
      localStorage.setItem("loom-last-agent", draftAgent);
      if (newMode === "auto") localStorage.setItem(`loom-auto-confirmed:${projectPath}:${created.id}`, "1");
      localStorage.removeItem(`loom-auto-confirmed:${projectPath}:new`);
      setNewMode("explore");
      setDrafts(previous => ({ ...previous, [JSON.stringify([projectPath, created.id])]: submitted,
        [draftKey]: previous[draftKey] === submitted ? "" : previous[draftKey] }));
      changed();
      if (!isCurrent()) return;
      firstSend.current = { project: projectPath, id: created.id };
      setSelection({ project: projectPath, id: created.id });
    } catch (error) { if (isCurrent()) setError(String(error)); }
    finally { createLock.current = false; setCreating(false); }
  }
  async function send() {
    if (!session || !projectPath || !draft.trim() || running || !listenReady) return;
    const key = JSON.stringify([projectPath, session.id]);
    if (pending.current.has(key)) return;
    pending.current.add(key); setPendingKeys(new Set(pending.current)); setError(null);
    const submitted = draft;
    try {
      let turnToAbort: string | undefined;
      if (!useBackend) setSession(mockChatStore.send(session.id, submitted) ?? null);
      else {
        const result = await chatSendController.send({ projectPath, sessionId: session.id, text: submitted, permissionMode: session.permissionMode });
        if (isCurrent()) setSession(result.session);
        if (pendingStops.current.delete(key)) { turnToAbort = result.turnId; setStopKeys(new Set(pendingStops.current)); }
      }
      setDrafts(previous => previous[draftKey] === submitted ? { ...previous, [draftKey]: "" } : previous);
      if (turnToAbort) await chatAbort({ projectPath, sessionId: session.id, turnId: turnToAbort });
      changed();
    } catch (error) { if (isCurrent()) setError(String(error)); }
    finally { pending.current.delete(key); pendingStops.current.delete(key); setPendingKeys(new Set(pending.current)); setStopKeys(new Set(pendingStops.current)); }
  }
  useEffect(() => {
    if (session && listenReady && firstSend.current?.id === session.id && firstSend.current.project === projectPath) {
      firstSend.current = null; void send();
    }
  });
  async function abort() {
    if (!session || !projectPath || !useBackend) return;
    const key = JSON.stringify([projectPath, session.id]);
    pendingStops.current.add(key); setStopKeys(new Set(pendingStops.current));
    if (!session.activeTurnId) return;
    try { await chatAbort({ projectPath, sessionId: session.id, turnId: session.activeTurnId }); }
    catch (error) { if (isCurrent()) setError(String(error)); }
    finally { pendingStops.current.delete(key); setStopKeys(new Set(pendingStops.current)); }
  }
  async function mutate(operation: () => Promise<ChatSession | null>) {
    try { const next = await operation(); if (isCurrent()) setSession(next); changed(); }
    catch (error) { if (isCurrent()) setError(String(error)); }
  }
  async function changeMode(mode: ChatPermissionMode) {
    if (running) return;
    if (!session || !projectPath) { setNewMode(mode); return; }
    await mutate(async () => useBackend ? chatUpdateMeta({ projectPath, sessionId: session.id, permissionMode: mode })
      : mockChatStore.setPermissionMode(session.id, mode) ?? null);
  }
  /** Auto-run asks once per chat; the answer is kept so later toggles stay quiet. */
  function requestMode(mode: ChatPermissionMode) {
    if (running) return;
    const key = `loom-auto-confirmed:${projectPath}:${sessionId ?? "new"}`;
    if (mode !== "auto" || localStorage.getItem(key)) { void changeMode(mode); return; }
    setConfirmation({ title: t("autoTitle"), body: t("autoWarning"), confirmLabel: t("autoConfirm"), onCancel: () => setConfirmation(null),
      onConfirm: () => { localStorage.setItem(key, "1"); setConfirmation(null); void changeMode("auto"); } });
  }
  async function changeAgent(id: string) {
    if (running) return;
    const target = agents.find(agent => agent.id === id && agent.available);
    if (!target || diagnostics.some(item => item.agentId === id && ["missing", "unavailable"].includes(item.status))) { setError(t("noAgent")); return; }
    if (!session || !projectPath) { setNewAgent(id); return; }
    if (id === session.agentId) return;
    setConfirmation({ title: t("agentSwitchTitle", { agent: target.name }), body: t("agentSwitchWarning"), confirmLabel: t("agentSwitchConfirm"), onCancel: () => setConfirmation(null),
      onConfirm: () => { setConfirmation(null); void mutate(async () => useBackend ? chatSetAgent(projectPath, session.id, id) : mockChatStore.setAgent(session.id, id) ?? null); } });
  }
  async function clearResume() {
    if (!session || !projectPath || running) return;
    await mutate(async () => useBackend ? chatClearResume(projectPath, session.id) : { ...session, resumeHandle: undefined, resumeCommand: undefined });
  }
  async function command(raw: string) {
    if (running) return;
    const parsed = parseSlashCommand(raw);
    if (!parsed) { setError(t("invalidCommand")); return; }
    if (parsed.command === "agent" && !agents.some(agent => agent.id === parsed.argument && agent.available)) { setError(t("noAgent")); return; }
    setDraft("");
    if (parsed.command === "new") { setNewMode("explore"); localStorage.removeItem(`loom-auto-confirmed:${projectPath}:new`); setSelection({ project: projectPath, id: null }); }
    else if (parsed.command === "agent") await changeAgent(parsed.argument);
    else if (parsed.command === "mode") requestMode(toPermissionMode(parsed.argument === "research" ? "research" : "code", parsed.argument === "auto")); else if (parsed.command === "clear-resume") await clearResume();
    else if (parsed.command === "export") await exportState.exportSession();
  }
  const selectedDiagnostic = diagnostics.find(item => item.agentId === session?.agentId);
  return <div className={`chat-page${collapsed ? " sidebar-collapsed" : ""}`} data-testid="chat-page">
    {!collapsed && <ProjectTree projects={state.projects.recent} agents={state.agents} projectPath={projectPath} sessionId={sessionId}
      useBackend={useBackend} revision={revision} onSelect={select} onAdd={() => setAddProjectOpen(true)}
      onSettings={() => dispatch({ type: "app/viewSelected", view: "settings" })} onCollapse={() => setCollapsed(true)} />}
    <main className="chat-main">
      <div className="chat-window-bar" data-tauri-drag-region>
        {collapsed && <button type="button" aria-label={t("expandSidebar")} onClick={() => setCollapsed(false)}><PanelLeftOpen size={16} /></button>}
        <UpdateButton />
      </div>
      {!project ? <div className="chat-empty-session"><div className="chat-empty-inner chat-no-project"><span className="chat-empty-mark"><LoomMark size={26} /></span>
        <h1>{t("noProject")}</h1><p>{t("noProjectHint")}</p>
        <button type="button" className="chat-primary-button" onClick={() => setAddProjectOpen(true)}><FolderPlus size={15} aria-hidden="true" />{t("addProject")}</button></div></div> : <>
        {session ? <>
          <ChatSessionHeader session={session} projectName={project.name} running={running} onMode={requestMode}
            onRename={title => void mutate(async () => useBackend ? chatUpdateMeta({ projectPath: project.path, sessionId: session.id, title }) : mockChatStore.setTitle(session.id, title) ?? null)}
            onArchive={() => void mutate(async () => {
              const status = session.status === "archived" ? "active" : "archived";
              return useBackend ? chatUpdateMeta({ projectPath: project.path, sessionId: session.id, status }) : mockChatStore.setStatus(session.id, status) ?? null;
            })} onClearResume={() => void clearResume()} onExport={() => void exportState.exportSession()} exporting={exportState.busy} />
          <ChatExportNotice busy={exportState.busy} error={exportState.error} result={exportState.result} />
          <ErrorBoundary resetKey={currentKey}><ChatTranscript messages={session.messages} turns={session.turns} projectPath={project.path} sessionId={session.id} toolEvents={selectedDiagnostic?.capabilities?.toolEvents !== false} inlineCards={inlineCards} onInsertText={insertText} /></ErrorBoundary>
        </> : sessionId ? <p className="chat-loading" role="status">{error ?? t("loading")}</p> : <EmptySession mode={newMode} onChange={setNewMode} projectName={project.name} projectPath={project.path} />}
        {(!sessionId || session) && <ChatComposer key={draftKey} draft={draft} sending={running} aborting={stopKeys.has(currentKey)} canSend={listenReady && !creating && session?.status !== "archived"}
          error={error || state.app.agentError} useBackend={useBackend} agentId={session?.agentId ?? draftAgent} agents={agents}
          permissionMode={session?.permissionMode ?? newMode} resumeHint={Boolean(session?.resumeHandle)} diagnostics={diagnostics}
          diagnosticsLoading={diagnosticsLoading} onRefreshDiagnostics={() => void refreshDiagnostics()} onDraftChange={setDraft}
          onSend={() => void (session ? send() : createAndSend())} onAbort={() => void abort()} onAgentChange={id => void changeAgent(id)}
          onPermissionChange={requestMode} confirmation={confirmation} onCommand={raw => void command(raw)} sessionKey={`${projectPath}:${sessionId ?? "new"}`} />}
      </>}
    </main>
    {addProjectOpen && <AddProjectModal onClose={() => setAddProjectOpen(false)} />}
  </div>;
}
export default ChatPage;
