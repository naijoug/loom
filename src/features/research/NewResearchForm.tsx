import { useState, type FormEvent } from "react";
import { LoaderCircle, Telescope } from "lucide-react";
import {
  RESEARCH_ADAPTERS,
  type CreateResearchInput,
  type ResearchApi,
  type ResearchLanguage,
} from "../../api/research";
import type { AgentConfig, AgentDiagnostic, ProjectSummary } from "../../domain";
import { AgentPicker, agentSupportsModel, defaultAgentId, isAgentUnusable } from "../../components/AgentPicker";
import { useLocale, useT } from "../../i18n";
import { ErrorText } from "../../components/common/ErrorText";

export function researchAgents(agents: AgentConfig[]) {
  return agents.filter((agent) => agent.enabled && RESEARCH_ADAPTERS.includes(agent.adapterType));
}

export function NewResearchForm({
  api,
  agents,
  diagnostics,
  projects,
  onCreated,
}: {
  api: ResearchApi;
  agents: AgentConfig[];
  diagnostics: AgentDiagnostic[];
  projects: ProjectSummary[];
  onCreated: (researchId: string) => void;
}) {
  const t = useT();
  const { locale } = useLocale();
  const eligible = researchAgents(agents);
  const [question, setQuestion] = useState("");
  const [agentId, setAgentId] = useState(() => defaultAgentId(eligible));
  const [model, setModel] = useState("");
  const [projectPath, setProjectPath] = useState("");
  const [language, setLanguage] = useState<ResearchLanguage>(locale === "zh-CN" ? "zh-CN" : "en");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const modelInvalid =
    model.trim() !== "" && (/\s/.test(model.trim()) || model.trim().startsWith("-") || model.trim().length > 128);
  const selected = eligible.find((agent) => agent.id === agentId);
  const usable =
    !!selected?.available && !isAgentUnusable(diagnostics.find((item) => item.agentId === agentId)?.status);
  // The model field shows only for agents that accept one; a value typed for another agent is not sent.
  const modelShown = agentSupportsModel(diagnostics, agentId);
  const canSubmit = !busy && question.trim() !== "" && !(modelShown && modelInvalid) && usable;
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!canSubmit) return;
    setBusy(true);
    setError("");
    const input: CreateResearchInput = {
      question: question.trim(),
      agentId,
      model: modelShown ? model.trim() || undefined : undefined,
      projectPath: projectPath || undefined,
      language,
    };
    try {
      onCreated((await api.create(input)).researchId);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }
  return (
    <form className="research-new" onSubmit={submit} aria-labelledby="research-new-title">
      <span className="chat-empty-mark">
        <Telescope size={24} />
      </span>
      <h1 id="research-new-title">{t("researchNewTitle")}</h1>
      <p className="research-note">{t("researchNewDesc")}</p>
      <label>
        {t("researchQuestion")}
        <textarea
          required
          autoFocus
          rows={4}
          placeholder={t("researchQuestionPlaceholder")}
          value={question}
          maxLength={8000}
          onChange={(event) => setQuestion(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) void submit(event);
          }}
        />
      </label>
      <div className="research-new-agent" role="group" aria-label={t("researchAgent")}>
        <span>{t("researchAgent")}</span>
        <div className="research-new-agent-controls">
          <AgentPicker
            agents={eligible}
            diagnostics={diagnostics}
            value={agentId}
            onChange={setAgentId}
            disabled={busy}
            placement="down"
            model={{ value: model, onChange: setModel, invalid: modelInvalid }}
          />
        </div>
        {!eligible.length && <small className="research-note">{t("researchNoAgent")}</small>}
      </div>
      <div className="research-new-row">
        <label>
          {t("researchProject")}
          <select value={projectPath} onChange={(event) => setProjectPath(event.target.value)}>
            <option value="">{t("researchNoProject")}</option>
            {projects.map((project) => (
              <option key={project.path} value={project.path}>
                {project.name}
              </option>
            ))}
          </select>
        </label>
        <label>
          {t("researchLanguage")}
          <select value={language} onChange={(event) => setLanguage(event.target.value as ResearchLanguage)}>
            <option value="zh-CN">中文</option>
            <option value="en">English</option>
          </select>
        </label>
      </div>
      <p className="research-note">
        {t("researchProjectHint")}
        {modelShown ? ` ${t("researchModelHint")}` : ""}
      </p>
      {error && <ErrorText text={error} alert className="research-error" />}
      <button type="submit" className="chat-primary-button" disabled={!canSubmit}>
        {busy && <LoaderCircle size={14} className="chat-spin" aria-hidden="true" />}
        {t("researchStart")}
      </button>
    </form>
  );
}
