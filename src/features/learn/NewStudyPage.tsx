import { useEffect, useRef, useState } from "react";
import { Sparkles } from "lucide-react";
import type { LearnApi } from "../../api/learn";
import type { AgentConfig, AgentDiagnostic, ModelProvider } from "../../domain";
import { useT } from "../../i18n";
import { DiscoverChat, type DiscoverChatState } from "./DiscoverChat";
import { NewStudyForm } from "./NewStudyForm";
import { TrendingList } from "./TrendingList";

/**
 * Learn start page: the new-study form and rising GitHub repositories on the left,
 * the Discover chat on the right (a toggled panel in narrow windows).
 */
export function NewStudyPage({
  api,
  agents,
  diagnostics,
  providers,
  chat,
  onCreated,
}: {
  api: LearnApi;
  agents: AgentConfig[];
  diagnostics: AgentDiagnostic[];
  providers: ModelProvider[];
  chat: DiscoverChatState;
  onCreated: (studyId: string) => void;
}) {
  const t = useT();
  const [url, setUrl] = useState("");
  const [discoverOpen, setDiscoverOpen] = useState(false);
  const urlInput = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!discoverOpen) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !event.defaultPrevented) setDiscoverOpen(false);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [discoverOpen]);
  function study(next: string) {
    setUrl(next);
    setDiscoverOpen(false);
    const input = urlInput.current;
    input?.scrollIntoView({ block: "nearest", behavior: "smooth" });
    input?.focus();
  }
  return (
    <div className="learn-start" data-discover-open={discoverOpen || undefined}>
      <button
        type="button"
        className="learn-discover-toggle"
        aria-expanded={discoverOpen}
        onClick={() => setDiscoverOpen(true)}
      >
        <Sparkles size={14} aria-hidden="true" />
        {t("learnDiscoverToggle")}
      </button>
      <div className="learn-start-main">
        <NewStudyForm
          api={api}
          agents={agents}
          diagnostics={diagnostics}
          providers={providers}
          url={url}
          onUrlChange={setUrl}
          urlInput={urlInput}
          onCreated={onCreated}
        />
        <TrendingList api={api} selectedUrl={url} onStudy={study} />
      </div>
      <DiscoverChat
        chat={chat}
        agents={agents}
        diagnostics={diagnostics}
        providers={providers}
        selectedUrl={url}
        onStudy={study}
        onClose={() => setDiscoverOpen(false)}
      />
    </div>
  );
}
