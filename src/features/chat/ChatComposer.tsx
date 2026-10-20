import { useEffect, useState } from "react";
import { ArrowUp, Square } from "lucide-react";
import type { AgentConfig, AgentDiagnostic, ChatPermissionMode } from "../../domain";
import { useT } from "../../i18n";
import { AgentPicker } from "./AgentPicker";
import { StatusLine } from "./StatusLine";
import { SlashMenu } from "./SlashMenu";
import { filterSlashCommands } from "./slashCommands";
import { toWorkMode } from "./workMode";

export interface ChatComposerProps {
  draft: string; sending: boolean; aborting?: boolean; canSend?: boolean;
  elapsedHint?: string | null; error: string | null; useBackend: boolean;
  agentId: string; agents: AgentConfig[]; permissionMode: ChatPermissionMode;
  resumeHint: boolean; diagnostics: AgentDiagnostic[]; diagnosticsLoading: boolean;
  onRefreshDiagnostics: () => void; onDraftChange: (value: string) => void;
  onSend: () => void; onAbort: () => void; onAgentChange: (id: string) => void;
  onPermissionChange: (mode: ChatPermissionMode) => void;
  onCommand?: (command: string) => void; sessionKey?: string;
}
export function ChatComposer(props: ChatComposerProps) {
  const { draft, sending, aborting, canSend = true, agentId, agents, permissionMode, diagnostics,
    onDraftChange, onSend, onAbort, onAgentChange, onPermissionChange, onCommand, sessionKey } = props;
  const t = useT();
  const [askConfirm, setAskConfirm] = useState(false);
  const [selectedCommand, setSelectedCommand] = useState(0);
  const [menuClosed, setMenuClosed] = useState(false);
  const mode = toWorkMode(permissionMode);
  const commands = menuClosed ? [] : filterSlashCommands(draft);
  const commandIndex = Math.min(selectedCommand, Math.max(0, commands.length - 1));
  const selected = agents.find(agent => agent.id === agentId);
  const diagnostic = diagnostics.find(item => item.agentId === agentId);
  const allowed = canSend && selected?.available && selected?.enabled && !["missing", "unavailable"].includes(diagnostic?.status ?? "");
  useEffect(() => { setAskConfirm(false); }, [sessionKey, permissionMode, sending]);
  function send() {
    if (sending || !draft.trim()) return;
    if (draft.startsWith("/") && onCommand) { onCommand(draft); return; }
    if (!allowed) return;
    if (permissionMode === "ask" && !askConfirm) { setAskConfirm(true); return; }
    setAskConfirm(false); onSend();
  }
  function choose(command: string) {
    if (command === "/agent" || command === "/mode") {
      onDraftChange(`${command} `); setMenuClosed(true);
    } else { onCommand?.(command); setMenuClosed(true); }
  }
  function auto(enabled: boolean) {
    if (sending) return;
    const key = `loom-auto-confirmed:${sessionKey ?? "new"}`;
    if (enabled && !localStorage.getItem(key)) {
      if (!window.confirm(t("autoWarning"))) return;
      localStorage.setItem(key, "1");
    }
    onPermissionChange(enabled ? "auto" : "ask");
  }
  return <div className="chat-composer-shell">
    {props.error && <p className="chat-error" role="alert">{props.error}</p>}
    <div className="chat-composer" data-mode={mode} onKeyDown={event => {
      if (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229) return;
      if (event.key === "Escape") {
        event.preventDefault();
        if (commands.length) setMenuClosed(true);
        else if (askConfirm) setAskConfirm(false);
        else if (sending && !aborting) onAbort();
      }
    }}>
      {commands.length > 0 && <SlashMenu commands={commands} selected={commandIndex} onSelect={choose} />}
      {askConfirm && <div className="chat-ask-confirm" role="dialog" aria-label={t("confirmTurn")}>
        <p>{t("confirmTurnHint")}</p><button type="button" onClick={() => setAskConfirm(false)}>{t("cancel")}</button>
        <button type="button" onClick={send}>{t("confirmSend")}</button>
      </div>}
      <textarea value={draft} readOnly={sending} aria-label={t("messageInput")} placeholder={t(mode === "research" ? "messageResearch" : "messageCode")}
        aria-controls={commands.length ? "chat-slash-menu" : undefined} aria-activedescendant={commands.length ? `chat-slash-${commandIndex}` : undefined}
        onChange={event => { onDraftChange(event.target.value); setMenuClosed(false); setSelectedCommand(0); }}
        onKeyDown={event => {
          if (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229) return;
          if (commands.length && ["ArrowDown", "ArrowUp"].includes(event.key)) {
            event.preventDefault(); setSelectedCommand((commandIndex + (event.key === "ArrowDown" ? 1 : -1) + commands.length) % commands.length);
          } else if (event.key === "Enter" && !event.shiftKey) {
            event.preventDefault(); if (commands.length) choose(commands[commandIndex]); else send();
          }
        }} />
      <div className="chat-composer-actions">
        <AgentPicker agents={agents} diagnostics={diagnostics} value={agentId} onChange={onAgentChange} disabled={sending} />
        {mode === "research" ? <span className="chat-readonly">{t("readOnly")}</span> :
          <label className="chat-auto"><input type="checkbox" checked={permissionMode === "auto"} disabled={sending} onChange={event => auto(event.target.checked)} />{t("autoRun")}</label>}
        <button type="button" className="chat-send" aria-label={t(sending ? "stop" : "send")} title={sending ? "Esc" : "Enter"}
          disabled={sending ? aborting : !draft.trim() || (!allowed && !draft.startsWith("/"))}
          onClick={sending ? onAbort : send}>{sending ? <Square size={14} fill="currentColor" /> : <ArrowUp size={17} />}</button>
      </div>
    </div>
    <StatusLine running={sending} resume={props.resumeHint} diagnostic={diagnostic} detail={[mode, props.elapsedHint].filter(Boolean).join(" · ")} />
  </div>;
}
