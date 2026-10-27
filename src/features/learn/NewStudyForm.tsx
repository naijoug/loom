import { useState, type FormEvent } from "react";
import { GraduationCap, LoaderCircle } from "lucide-react";
import { LEARN_ADAPTERS, type CreateStudyInput, type LearnApi, type LearnDepth, type LearnLanguage } from "../../api/learn";
import type { AgentConfig } from "../../domain";
import { useLocale, useT } from "../../i18n";
import { ErrorText } from "../../components/common/ErrorText";

const GITHUB_URL = /^https:\/\/github\.com\/[A-Za-z0-9-]+\/[A-Za-z0-9._-]+?(\.git)?\/?$/;

export function learnAgents(agents: AgentConfig[]) {
  return agents.filter(agent => agent.enabled && LEARN_ADAPTERS.includes(agent.adapterType));
}

export function NewStudyForm({ api, agents, onCreated }: { api: LearnApi; agents: AgentConfig[]; onCreated: (studyId: string) => void }) {
  const t = useT();
  const { locale } = useLocale();
  const eligible = learnAgents(agents);
  const [url, setUrl] = useState("");
  const [gitRef, setGitRef] = useState("");
  const [agentId, setAgentId] = useState(() => eligible.find(agent => agent.available && agent.adapterType === "claude_code_cli")?.id ?? eligible.find(agent => agent.available)?.id ?? "");
  const [model, setModel] = useState("");
  const [language, setLanguage] = useState<LearnLanguage>(locale === "zh-CN" ? "zh-CN" : "en");
  const [depth, setDepth] = useState<LearnDepth>("standard");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const trimmed = url.trim();
  const urlInvalid = trimmed !== "" && !GITHUB_URL.test(trimmed);
  const modelInvalid = model.trim() !== "" && (/\s/.test(model.trim()) || model.trim().startsWith("-") || model.trim().length > 128);
  const selected = eligible.find(agent => agent.id === agentId);
  const canSubmit = !busy && trimmed !== "" && !urlInvalid && !modelInvalid && !!selected?.available;
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!canSubmit) return;
    setBusy(true); setError("");
    const input: CreateStudyInput = { url: trimmed, gitRef: gitRef.trim() || undefined, agentId, model: model.trim() || undefined, language, depth };
    try { onCreated((await api.create(input)).study.studyId); }
    catch (reason) { setError(String(reason)); }
    finally { setBusy(false); }
  }
  return <form className="learn-new" onSubmit={submit} aria-labelledby="learn-new-title">
    <span className="chat-empty-mark"><GraduationCap size={24} /></span>
    <h1 id="learn-new-title">{t("learnNewTitle")}</h1>
    <p className="learn-note">{t("learnNewDesc")}</p>
    <label>{t("learnRepoUrl")}
      <input type="url" required autoFocus placeholder="https://github.com/owner/repo" value={url} aria-invalid={urlInvalid || undefined} onChange={event => setUrl(event.target.value)} />
      {urlInvalid && <small className="learn-error">{t("learnRepoUrlInvalid")}</small>}
    </label>
    <div className="learn-new-row">
      <label>{t("learnAgent")}
        <select value={agentId} onChange={event => setAgentId(event.target.value)}>
          {!eligible.length && <option value="">{t("learnNoAgent")}</option>}
          {eligible.map(agent => <option key={agent.id} value={agent.id} disabled={!agent.available}>{agent.name}{agent.available ? "" : ` (${t("unavailable")})`}</option>)}
        </select>
      </label>
      <label>{t("learnModel")}
        <input value={model} placeholder={t("learnModelDefault")} spellCheck={false} aria-invalid={modelInvalid || undefined} onChange={event => setModel(event.target.value)} />
      </label>
    </div>
    <div className="learn-new-row">
      <label>{t("learnLanguage")}
        <select value={language} onChange={event => setLanguage(event.target.value as LearnLanguage)}>
          <option value="zh-CN">中文</option><option value="en">English</option>
        </select>
      </label>
      <label>{t("learnDepth")}
        <select value={depth} onChange={event => setDepth(event.target.value as LearnDepth)}>
          <option value="quick">{t("learnDepthQuick")}</option><option value="standard">{t("learnDepthStandard")}</option><option value="deep">{t("learnDepthDeep")}</option>
        </select>
      </label>
      <label>{t("learnRef")}
        <input value={gitRef} placeholder={t("learnRefDefault")} spellCheck={false} onChange={event => setGitRef(event.target.value)} />
      </label>
    </div>
    <p className="learn-note">{t("learnModelHint")}</p>
    {error && <ErrorText text={error} alert className="learn-error" />}
    <button type="submit" className="chat-primary-button" disabled={!canSubmit}>{busy && <LoaderCircle size={14} className="chat-spin" aria-hidden="true" />}{t("learnStart")}</button>
  </form>;
}
