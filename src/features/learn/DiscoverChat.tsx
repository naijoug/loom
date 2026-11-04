import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { ArrowUp, LoaderCircle, Sparkles, Square, Trash2, X } from "lucide-react";
import {
  DISCOVER_ADAPTERS,
  type DiscoverMessage,
  type GithubRepoCard,
  type LearnApi,
  type LearnLanguage,
} from "../../api/learn";
import type { AgentConfig, AgentDiagnostic, ModelProvider } from "../../domain";
import { AgentPicker, agentProvider, isAgentUnusable } from "../../components/AgentPicker";
import { ErrorText } from "../../components/common/ErrorText";
import { Markdown } from "../../components/Markdown";
import { useLocale, useT } from "../../i18n";
import { RepoCard, sameRepoUrl } from "./RepoCard";

export interface DiscoverEntry {
  id: string;
  role: "user" | "assistant";
  text: string;
  repos?: GithubRepoCard[];
  queries?: string[];
  warnings?: string[];
  error?: string;
  stopped?: boolean;
}

export function discoverAgents(agents: AgentConfig[]) {
  const eligible = agents.filter((agent) => agent.enabled && DISCOVER_ADAPTERS.includes(agent.adapterType));
  return [...eligible.filter((a) => a.adapterType !== "api"), ...eligible.filter((a) => a.adapterType === "api")];
}

/** What the agent sees of earlier turns: visible text plus the repositories Loom found. Failed turns are left out. */
export function discoverHistory(entries: DiscoverEntry[]): DiscoverMessage[] {
  return entries
    .filter((entry) => !entry.error && !entry.stopped && entry.text.trim())
    .map((entry) => ({
      role: entry.role,
      text: entry.repos?.length
        ? `${entry.text}\n\n(Found on GitHub: ${entry.repos.map((repo) => repo.fullName).join(", ")})`
        : entry.text,
    }));
}

let sequence = 0;
const nextId = () => `discover-${Date.now()}-${++sequence}`;

/**
 * Conversation state for the Discover panel. It lives in LearnPage, so opening a study and
 * coming back keeps the conversation and any turn still running.
 */
export function useDiscoverChat(api: LearnApi) {
  const [entries, setEntries] = useState<DiscoverEntry[]>([]);
  const [running, setRunning] = useState<{ runId: string; agentName: string } | null>(null);
  const runningRef = useRef<string | null>(null);
  async function send(text: string, agent: AgentConfig, language: LearnLanguage) {
    const prompt = text.trim();
    if (!prompt || runningRef.current) return;
    const runId = nextId();
    const user: DiscoverEntry = { id: `${runId}-u`, role: "user", text: prompt };
    const history = discoverHistory([...entries, user]);
    setEntries((list) => [...list, user]);
    runningRef.current = runId;
    setRunning({ runId, agentName: agent.name });
    let reply: DiscoverEntry;
    try {
      const result = await api.discoverSend({ runId, agentId: agent.id, language, messages: history });
      reply = {
        id: runId,
        role: "assistant",
        text: result.reply,
        repos: result.repos,
        queries: result.queries,
        warnings: result.warnings,
      };
    } catch (reason) {
      const message = String(reason);
      reply = /\bStopped\b/.test(message)
        ? { id: runId, role: "assistant", text: "", stopped: true }
        : { id: runId, role: "assistant", text: "", error: message };
    }
    if (runningRef.current !== runId) return; // cleared while running
    runningRef.current = null;
    setRunning(null);
    setEntries((list) => [...list, reply]);
  }
  function stop() {
    if (running) void api.discoverCancel(running.runId);
  }
  function clear() {
    if (runningRef.current) void api.discoverCancel(runningRef.current);
    runningRef.current = null;
    setRunning(null);
    setEntries([]);
  }
  return { entries, running, send, stop, clear };
}
export type DiscoverChatState = ReturnType<typeof useDiscoverChat>;

function storedAgent() {
  try {
    return localStorage.getItem("loom-learn-discover-agent");
  } catch {
    return null;
  }
}

export function DiscoverChat({
  chat,
  agents,
  diagnostics,
  providers,
  selectedUrl,
  onStudy,
  onClose,
}: {
  chat: DiscoverChatState;
  agents: AgentConfig[];
  diagnostics: AgentDiagnostic[];
  providers: ModelProvider[];
  selectedUrl: string;
  onStudy: (url: string) => void;
  onClose?: () => void;
}) {
  const t = useT();
  const { locale } = useLocale();
  const eligible = discoverAgents(agents);
  const [agentId, setAgentId] = useState(() => {
    const saved = storedAgent();
    return (
      eligible.find((agent) => agent.id === saved && agent.available)?.id ??
      eligible.find((agent) => agent.available && agent.adapterType === "claude_code_cli")?.id ??
      eligible.find((agent) => agent.available)?.id ??
      ""
    );
  });
  const [draft, setDraft] = useState("");
  const list = useRef<HTMLDivElement>(null);
  const input = useRef<HTMLTextAreaElement>(null);
  const selected = eligible.find((agent) => agent.id === agentId);
  const usable =
    !!selected?.available &&
    !isAgentUnusable(diagnostics.find((item) => item.agentId === agentId)?.status) &&
    agentProvider(selected, providers)?.configured !== false;
  const busy = !!chat.running;
  const canSend = usable && !busy && draft.trim() !== "";
  // A new answer scrolls to its start; anything else (a sent message, the thinking line) to the bottom.
  useEffect(() => {
    const element = list.current;
    if (!element) return;
    const last = chat.entries[chat.entries.length - 1];
    const answers = element.querySelectorAll<HTMLElement>(".learn-discover-answer");
    const answer = last?.role === "assistant" && !busy ? answers[answers.length - 1] : undefined;
    if (answer) element.scrollTop = answer.offsetTop - 8;
    else element.scrollTop = element.scrollHeight;
  }, [chat.entries, busy]);
  function pickAgent(id: string) {
    setAgentId(id);
    try {
      localStorage.setItem("loom-learn-discover-agent", id);
    } catch {
      /* per-device convenience only */
    }
  }
  function submit(text = draft) {
    if (!selected || !usable || busy || !text.trim()) return;
    setDraft("");
    void chat.send(text, selected, locale === "zh-CN" ? "zh-CN" : "en");
    input.current?.focus();
  }
  function onKeyDown(event: KeyboardEvent<HTMLTextAreaElement>) {
    if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
      event.preventDefault();
      submit();
    }
  }
  const examples = [
    t("learnDiscoverExample1"),
    t("learnDiscoverExample2"),
    t("learnDiscoverExample3"),
    t("learnDiscoverExample4"),
  ];
  return (
    <aside className="learn-discover" aria-labelledby="learn-discover-title">
      <header className="learn-discover-head">
        <h2 id="learn-discover-title">
          <Sparkles size={15} aria-hidden="true" />
          {t("learnDiscoverTitle")}
        </h2>
        <button
          type="button"
          className="chat-icon-button"
          aria-label={t("learnDiscoverClear")}
          title={t("learnDiscoverClear")}
          disabled={!chat.entries.length && !busy}
          onClick={chat.clear}
        >
          <Trash2 size={14} />
        </button>
        {onClose && (
          <button
            type="button"
            className="chat-icon-button learn-discover-close"
            aria-label={t("learnDiscoverClose")}
            title={t("learnDiscoverClose")}
            onClick={onClose}
          >
            <X size={15} />
          </button>
        )}
      </header>
      <div className="learn-discover-list" ref={list} aria-live="polite">
        {!chat.entries.length && (
          <div className="learn-discover-empty">
            <p className="learn-note">{t("learnDiscoverDesc")}</p>
            <div className="learn-discover-examples">
              {examples.map((example) => (
                <button key={example} type="button" disabled={!usable || busy} onClick={() => submit(example)}>
                  {example}
                </button>
              ))}
            </div>
          </div>
        )}
        {chat.entries.map((entry) =>
          entry.role === "user" ? (
            <div key={entry.id} className="learn-discover-user" aria-label={t("learnDiscoverYou")}>
              {entry.text}
            </div>
          ) : (
            <div key={entry.id} className="learn-discover-answer">
              {entry.error && <ErrorText text={entry.error} className="learn-error" />}
              {entry.stopped && <p className="learn-note">{t("learnDiscoverStopped")}</p>}
              {entry.text && <Markdown text={entry.text} />}
              {!!entry.repos?.length && (
                <div className="learn-discover-repos">
                  {entry.repos.map((repo) => (
                    <RepoCard
                      key={repo.fullName}
                      repo={repo}
                      compact
                      selected={sameRepoUrl(selectedUrl, repo.url)}
                      onStudy={onStudy}
                    />
                  ))}
                </div>
              )}
              {!!entry.queries?.length && (
                <p className="learn-note">{t("learnDiscoverSearched", { queries: entry.queries.join(" · ") })}</p>
              )}
              {entry.warnings?.map((warning) => (
                <p key={warning} className="learn-discover-warning">
                  {warning}
                </p>
              ))}
            </div>
          ),
        )}
        {chat.running && (
          <p className="learn-note learn-discover-thinking">
            <LoaderCircle size={13} className="chat-spin" aria-hidden="true" />
            {t("learnDiscoverThinking", { agent: chat.running.agentName })}
          </p>
        )}
      </div>
      <form
        className="learn-discover-composer"
        onSubmit={(event) => {
          event.preventDefault();
          submit();
        }}
      >
        <textarea
          ref={input}
          rows={2}
          value={draft}
          placeholder={t("learnDiscoverPlaceholder")}
          aria-label={t("learnDiscoverPlaceholder")}
          onChange={(event) => setDraft(event.target.value)}
          onKeyDown={onKeyDown}
        />
        <div className="learn-discover-composer-bar">
          <AgentPicker
            agents={eligible}
            diagnostics={diagnostics}
            providers={providers}
            value={agentId}
            onChange={pickAgent}
            disabled={busy}
          />
          {busy ? (
            <button
              type="button"
              className="learn-discover-send"
              aria-label={t("learnDiscoverStop")}
              title={t("learnDiscoverStop")}
              onClick={chat.stop}
            >
              <Square size={12} fill="currentColor" />
            </button>
          ) : (
            <button
              type="submit"
              className="learn-discover-send"
              aria-label={t("learnDiscoverSend")}
              title={t("learnDiscoverSend")}
              disabled={!canSend}
            >
              <ArrowUp size={15} />
            </button>
          )}
        </div>
        <small className="learn-note">
          {eligible.length ? t("learnDiscoverHint", { agent: selected?.name ?? "Agent" }) : t("learnDiscoverNoAgent")}
        </small>
      </form>
    </aside>
  );
}
