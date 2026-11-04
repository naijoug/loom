import { useState, type FormEvent, type Ref } from "react";
import { GraduationCap, LoaderCircle } from "lucide-react";
import {
  LEARN_ADAPTERS,
  type CreateStudyInput,
  type LearnApi,
  type LearnDepth,
  type LearnLanguage,
} from "../../api/learn";
import type { AgentConfig, AgentDiagnostic, ModelProvider } from "../../domain";
import {
  AgentPicker,
  agentProvider,
  agentSupportsModel,
  isAgentUnusable,
  pickerAgents,
} from "../../components/AgentPicker";
import { useLocale, useT } from "../../i18n";
import { ErrorText } from "../../components/common/ErrorText";

const GITHUB_URL = /^https:\/\/github\.com\/[A-Za-z0-9-]+\/[A-Za-z0-9._-]+?(\.git)?\/?$/;

export function learnAgents(agents: AgentConfig[]) {
  return agents.filter((agent) => agent.enabled && LEARN_ADAPTERS.includes(agent.adapterType));
}

export function NewStudyForm({
  api,
  agents,
  diagnostics,
  providers = [],
  url,
  onUrlChange: setUrl,
  urlInput,
  onCreated,
}: {
  api: LearnApi;
  agents: AgentConfig[];
  diagnostics: AgentDiagnostic[];
  providers?: ModelProvider[];
  /** Controlled so repository cards on the start page can fill it in. */
  url: string;
  onUrlChange: (url: string) => void;
  urlInput?: Ref<HTMLInputElement>;
  onCreated: (studyId: string) => void;
}) {
  const t = useT();
  const { locale } = useLocale();
  const eligible = learnAgents(agents);
  const [gitRef, setGitRef] = useState("");
  const [agentId, setAgentId] = useState(
    () =>
      eligible.find((agent) => agent.available && agent.adapterType === "claude_code_cli")?.id ??
      eligible.find((agent) => agent.available)?.id ??
      "",
  );
  const [model, setModel] = useState("");
  const [language, setLanguage] = useState<LearnLanguage>(locale === "zh-CN" ? "zh-CN" : "en");
  const [depth, setDepth] = useState<LearnDepth>("standard");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const trimmed = url.trim();
  const urlInvalid = trimmed !== "" && !GITHUB_URL.test(trimmed);
  const modelInvalid =
    model.trim() !== "" && (/\s/.test(model.trim()) || model.trim().startsWith("-") || model.trim().length > 128);
  const selected = eligible.find((agent) => agent.id === agentId);
  const usable =
    !!selected?.available &&
    !isAgentUnusable(diagnostics.find((item) => item.agentId === agentId)?.status) &&
    agentProvider(selected, providers)?.configured !== false;
  // The model field shows only for agents that accept one; a value typed for another agent is not sent.
  const modelShown = agentSupportsModel(diagnostics, agentId);
  const canSubmit = !busy && trimmed !== "" && !urlInvalid && !(modelShown && modelInvalid) && usable;
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!canSubmit) return;
    setBusy(true);
    setError("");
    const input: CreateStudyInput = {
      url: trimmed,
      gitRef: gitRef.trim() || undefined,
      agentId,
      model: modelShown ? model.trim() || undefined : undefined,
      language,
      depth,
    };
    try {
      onCreated((await api.create(input)).study.studyId);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }
  return (
    <form className="learn-new" onSubmit={submit} aria-labelledby="learn-new-title">
      <div className="learn-new-head">
        <span className="chat-empty-mark">
          <GraduationCap size={20} />
        </span>
        <div>
          <h1 id="learn-new-title">{t("learnNewTitle")}</h1>
          <p className="learn-note">{t("learnNewDesc")}</p>
        </div>
      </div>
      <label>
        {t("learnRepoUrl")}
        <span className="learn-new-url">
          <input
            ref={urlInput}
            type="url"
            required
            autoFocus
            placeholder="https://github.com/owner/repo"
            value={url}
            aria-invalid={urlInvalid || undefined}
            onChange={(event) => setUrl(event.target.value)}
          />
          <button type="submit" className="chat-primary-button" disabled={!canSubmit}>
            {busy && <LoaderCircle size={14} className="chat-spin" aria-hidden="true" />}
            {t("learnStart")}
          </button>
        </span>
        {urlInvalid && <small className="learn-error">{t("learnRepoUrlInvalid")}</small>}
      </label>
      <div className="learn-new-row">
        <div className="learn-new-agent" role="group" aria-label={t("learnAgent")}>
          <span>{t("learnAgent")}</span>
          <div className="learn-new-agent-controls">
            <AgentPicker
              agents={pickerAgents(eligible, agents)}
              diagnostics={diagnostics}
              providers={providers}
              needsTools
              value={agentId}
              onChange={setAgentId}
              disabled={busy}
              placement="down"
              model={{ value: model, onChange: setModel, invalid: modelInvalid }}
            />
          </div>
          {!eligible.length && <small className="learn-note">{t("learnNoAgent")}</small>}
        </div>
        <label>
          {t("learnLanguage")}
          <select value={language} onChange={(event) => setLanguage(event.target.value as LearnLanguage)}>
            <option value="zh-CN">中文</option>
            <option value="en">English</option>
          </select>
        </label>
        <label>
          {t("learnDepth")}
          <select value={depth} onChange={(event) => setDepth(event.target.value as LearnDepth)}>
            <option value="quick">{t("learnDepthQuick")}</option>
            <option value="standard">{t("learnDepthStandard")}</option>
            <option value="deep">{t("learnDepthDeep")}</option>
          </select>
        </label>
        <label>
          {t("learnRef")}
          <input
            value={gitRef}
            placeholder={t("learnRefDefault")}
            spellCheck={false}
            onChange={(event) => setGitRef(event.target.value)}
          />
        </label>
      </div>
      {modelShown && <p className="learn-note">{t("learnModelHint")}</p>}
      {error && <ErrorText text={error} alert className="learn-error" />}
    </form>
  );
}
