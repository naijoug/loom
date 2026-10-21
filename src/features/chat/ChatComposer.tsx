import { useEffect, useState } from "react";
import { ArrowUp, Lock, Square } from "lucide-react";
import type { AgentConfig, AgentDiagnostic, ChatPermissionMode } from "../../domain";
import { useT } from "../../i18n";
import { AgentPicker } from "./AgentPicker";
import { ConfirmBanner, type Confirmation } from "./ConfirmBanner";
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
  /** Pending Auto-run or Agent switch confirmation owned by the page. */
  confirmation?: Confirmation | null;
}
export function ChatComposer(props: ChatComposerProps) {
  const { draft, sending, aborting, canSend = true, agentId, agents, permissionMode, diagnostics,
    onDraftChange, onSend, onAbort, onAgentChange, onPermissionChange, onCommand, sessionKey, confirmation } = props;
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
  return <div className="chat-composer-shell">
    {props.error && <p className="chat-error chat-composer-error" role="alert">{props.error}</p>}
    <div className="chat-composer" data-mode={mode} data-auto={permissionMode === "auto"} onKeyDown={event => {
      if (event.nativeEvent.isComposing || event.nativeEvent.keyCode === 229) return;
      if (event.key === "Escape") {
        event.preventDefault();
        if (commands.length) setMenuClosed(true);
        else if (confirmation) confirmation.onCancel();
        else if (askConfirm) setAskConfirm(false);
        else if (sending && !aborting) onAbort();
      }
    }}>
      {commands.length > 0 && <SlashMenu commands={commands} selected={commandIndex} onSelect={choose} />}
      {confirmation && <ConfirmBanner {...confirmation} />}
      {askConfirm && !confirmation && <ConfirmBanner className="chat-ask-confirm" title={t("confirmTurn")} body={t("confirmTurnHint")}
        confirmLabel={t("confirmSend")} onConfirm={send} onCancel={() => setAskConfirm(false)} />}
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
        {mode === "research" ? <span className="chat-mode-hint" data-mode="research"><Lock size={12} aria-hidden="true" />{t("readOnly")}</span> : <>
          <label className="chat-auto">
            <input type="checkbox" role="switch" checked={permissionMode === "auto"} disabled={sending}
              onChange={event => onPermissionChange(event.target.checked ? "auto" : "ask")} />
            <span className="chat-switch" aria-hidden="true" />{t("autoRun")}
          </label>
          <span className="chat-mode-hint" data-mode="code">{t(permissionMode === "auto" ? "autoRunHint" : "askHint")}</span>
        </>}
        <button type="button" className="chat-send" data-running={sending} aria-label={t(sending ? "stop" : "send")} title={sending ? `${t("stop")} (Esc)` : `${t("send")} (Enter)`}
          disabled={sending ? aborting : !draft.trim() || (!allowed && !draft.startsWith("/"))}
          onClick={sending ? onAbort : send}>{sending ? <Square size={11} fill="currentColor" /> : <ArrowUp size={16} strokeWidth={2.4} />}</button>
      </div>
    </div>
    <StatusLine running={sending} resume={props.resumeHint} diagnostic={diagnostic} detail={[mode, props.elapsedHint].filter(Boolean).join(" · ")} />
  </div>;
}
