import { useRef, useState, type FormEvent } from "react";
import { LoaderCircle, Telescope } from "lucide-react";
import { RESEARCH_ADAPTERS, type CreateResearchInput, type ResearchApi, type ResearchLanguage } from "../../api/research";
import { researchRequestId, targetResearchApi, type TargetInput, type TargetPreflight } from "../../api/researchTargets";
import type { AgentConfig, AgentDiagnostic, ProjectSummary } from "../../domain";
import { AgentPicker, agentSupportsModel, defaultAgentId, isAgentUnusable } from "../../components/AgentPicker";
import { useLocale, useT } from "../../i18n";
import { ErrorText } from "../../components/common/ErrorText";

export function researchAgents(agents: AgentConfig[]) {
  return agents.filter(agent => agent.enabled && RESEARCH_ADAPTERS.includes(agent.adapterType));
}

type TargetKind = "github" | "website" | "app";

export function NewResearchForm({ api, targets, targetsEnabled, iterationEnabled, agents, diagnostics, projects, onCreated }: {
  api: ResearchApi; targets: typeof targetResearchApi; targetsEnabled: boolean; iterationEnabled: boolean;
  agents: AgentConfig[]; diagnostics: AgentDiagnostic[]; projects: ProjectSummary[]; onCreated: (researchId: string) => void;
}) {
  const t = useT();
  const { locale } = useLocale();
  const eligible = researchAgents(agents);
  const [mode, setMode] = useState<"question" | "target">("question");
  const [continuous, setContinuous] = useState(false);
  const [kind, setKind] = useState<TargetKind>("github");
  const [githubUrl, setGithubUrl] = useState("");
  const [gitRef, setGitRef] = useState("");
  const [websiteUrl, setWebsiteUrl] = useState("");
  const [appPath, setAppPath] = useState("");
  const [nodePath, setNodePath] = useState("");
  const [reaEntry, setReaEntry] = useState("");
  const [question, setQuestion] = useState("");
  const [agentId, setAgentId] = useState(() => defaultAgentId(eligible));
  const [model, setModel] = useState("");
  const [projectPath, setProjectPath] = useState("");
  const [language, setLanguage] = useState<ResearchLanguage>(locale === "zh-CN" ? "zh-CN" : "en");
  const [preflight, setPreflight] = useState<TargetPreflight | null>(null);
  const [preflightKey, setPreflightKey] = useState("");
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const [error, setError] = useState("");
  const targetMode = targetsEnabled && mode === "target";
  const modelInvalid = model.trim() !== "" && (/\s/.test(model.trim()) || model.trim().startsWith("-") || model.trim().length > 128);
  const selected = eligible.find(agent => agent.id === agentId);
  const usable = !!selected?.available && !isAgentUnusable(diagnostics.find(item => item.agentId === agentId)?.status);
  // The model field shows only for agents that accept one; a value typed for another agent is not sent.
  const modelShown = agentSupportsModel(diagnostics, agentId);
  const toolsReady = kind === "github" || (nodePath.trim() !== "" && reaEntry.trim() !== "");
  const targetFilled = kind === "github" ? githubUrl.trim() !== "" : kind === "website" ? websiteUrl.trim() !== "" : appPath.trim() !== "";
  const canSubmit = !busy && question.trim() !== "" && !(modelShown && modelInvalid) && usable && (!targetMode || (targetFilled && toolsReady));
  function spec(): TargetInput {
    const target = kind === "github"
      ? { kind: "github" as const, url: githubUrl.trim(), gitRef: gitRef.trim() || null }
      : kind === "website" ? { kind: "website" as const, url: websiteUrl.trim() }
      : { kind: "app" as const, path: appPath.trim() };
    return {
      target, question: question.trim(), agentId, model: modelShown ? model.trim() || null : null, language,
      mode: targetMode && iterationEnabled && continuous ? "iterative" : "single", budget: null, tools: kind === "github" ? null : { node: nodePath.trim(), entry: reaEntry.trim() },
    };
  }
  function clearPreflight() { setPreflight(null); setPreflightKey(""); }
  async function browseApp() {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selectedPath = await open({ directory: true, multiple: false, title: t("researchAppPath") });
      if (typeof selectedPath === "string") { setAppPath(selectedPath); clearPreflight(); }
    } catch (reason) { setError(String(reason)); }
  }
  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!canSubmit || busyRef.current) return;
    busyRef.current = true;
    setBusy(true); setError("");
    try {
      if (!targetMode) {
        const input: CreateResearchInput = { question: question.trim(), agentId, model: modelShown ? model.trim() || undefined : undefined, projectPath: projectPath || undefined, language };
        onCreated((await api.create(input)).researchId);
        return;
      }
      const next = spec();
      const key = JSON.stringify(next);
      if (preflight && preflightKey === key) {
        onCreated((await targets.start(preflight.preflightId, researchRequestId())).spec.researchId);
        return;
      }
      const checked = await targets.preflight(next);
      setPreflight(checked);
      setPreflightKey(key);
    } catch (reason) { setError(String(reason)); }
    finally { busyRef.current = false; setBusy(false); }
  }
  const checked = targetMode && preflight && preflightKey === JSON.stringify(spec()) ? preflight : null;
  return <form className="research-new" onSubmit={submit} aria-labelledby="research-new-title"
    onKeyDown={event => { if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) { event.preventDefault(); void submit(event); } }}>
    <span className="chat-empty-mark"><Telescope size={24} /></span>
    <h1 id="research-new-title">{t("researchNewTitle")}</h1>
    <p className="research-note">{t("researchNewDesc")}</p>
    {targetsEnabled && <fieldset className="research-mode" data-testid="research-target-form">
      <legend>{t("researchMode")}</legend>
      <label><input type="radio" name="research-mode" value="question" checked={!targetMode} disabled={busy} onChange={() => { setMode("question"); clearPreflight(); }} />{t("researchModeQuestion")}</label>
      <label><input type="radio" name="research-mode" value="target" checked={targetMode} disabled={busy} onChange={() => { setMode("target"); clearPreflight(); }} />{t("researchModeTarget")}</label>
      <p className="research-note">{t("researchModeTargetHint")}</p>
    </fieldset>}
    {targetMode && <>
      <label>{t("researchTargetKind")}
        <select id="research-target-kind" value={kind} disabled={busy} onChange={event => { setKind(event.target.value as TargetKind); clearPreflight(); }}>
          <option value="github">{t("researchTargetGithub")}</option>
          <option value="website">{t("researchTargetWebsite")}</option>
          <option value="app">{t("researchTargetApp")}</option>
        </select>
      </label>
      {kind === "github" && <div className="research-new-row">
        <label>{t("researchGithubUrl")}<input id="research-github-url" type="url" required value={githubUrl} disabled={busy} onChange={event => { setGithubUrl(event.target.value); clearPreflight(); }} /></label>
        <label>{t("researchGitRef")}<input id="research-git-ref" type="text" value={gitRef} disabled={busy} onChange={event => { setGitRef(event.target.value); clearPreflight(); }} /></label>
      </div>}
      {kind === "website" && <>
        <label>{t("researchWebsiteUrl")}<input id="research-website-url" type="url" required value={websiteUrl} disabled={busy} onChange={event => { setWebsiteUrl(event.target.value); clearPreflight(); }} /></label>
        <p className="research-note research-warning">{t("researchWebsiteWarning")}</p>
      </>}
      {kind === "app" && <label>{t("researchAppPath")}
        <span className="research-new-row">
          <input id="research-app-path" type="text" required value={appPath} disabled={busy} onChange={event => { setAppPath(event.target.value); clearPreflight(); }} />
          <button type="button" disabled={busy} onClick={() => void browseApp()}>{t("researchBrowse")}</button>
        </span>
      </label>}
      {kind !== "github" && <div className="research-new-row">
        <label>{t("researchReaNode")}<input id="research-rea-node" type="text" required value={nodePath} disabled={busy} onChange={event => { setNodePath(event.target.value); clearPreflight(); }} /></label>
        <label>{t("researchReaEntry")}<input id="research-rea-entry" type="text" required value={reaEntry} disabled={busy} onChange={event => { setReaEntry(event.target.value); clearPreflight(); }} /></label>
      </div>}
      <p className="research-note">{t("researchReaHint")}</p>
      {iterationEnabled && <label className="research-iteration"><input id="research-iteration" type="checkbox" checked={continuous} disabled={busy} onChange={event => { setContinuous(event.target.checked); clearPreflight(); }} />{t("researchIteration")}</label>}
      {iterationEnabled && <p className="research-note">{t("researchIterationHint")}</p>}
    </>}
    <label>{t("researchQuestion")}
      <textarea required autoFocus={!targetMode} rows={4} placeholder={t("researchQuestionPlaceholder")} value={question} maxLength={8000}
        onChange={event => { setQuestion(event.target.value); if (targetMode) clearPreflight(); }} />
    </label>
    <div className="research-new-agent" role="group" aria-label={t("researchAgent")}>
      <span>{t("researchAgent")}</span>
      <div className="research-new-agent-controls">
        <AgentPicker agents={eligible} diagnostics={diagnostics} value={agentId} onChange={id => { setAgentId(id); clearPreflight(); }} disabled={busy} placement="down"
          model={{ value: model, onChange: value => { setModel(value); clearPreflight(); }, invalid: modelInvalid }} />
      </div>
      {!eligible.length && <small className="research-note">{t("researchNoAgent")}</small>}
    </div>
    <div className="research-new-row">
      {!targetMode && <label>{t("researchProject")}
        <select value={projectPath} onChange={event => setProjectPath(event.target.value)}>
          <option value="">{t("researchNoProject")}</option>
          {projects.map(project => <option key={project.path} value={project.path}>{project.name}</option>)}
        </select>
      </label>}
      <label>{t("researchLanguage")}
        <select value={language} onChange={event => { setLanguage(event.target.value as ResearchLanguage); if (targetMode) clearPreflight(); }}>
          <option value="zh-CN">中文</option><option value="en">English</option>
        </select>
      </label>
    </div>
    <p className="research-note">{targetMode ? t("researchModeTargetHint") : t("researchProjectHint")}{modelShown ? ` ${t("researchModelHint")}` : ""}</p>
    {checked && <section className="research-preflight" aria-label={t("researchPreflightTitle")}>
      <h2>{t("researchPreflightTitle")}</h2>
      <h3>{t("researchOperations")}</h3>
      <ul>{checked.operations.map(item => <li key={item}>{item}</li>)}</ul>
      <h3>{t("researchLimitations")}</h3>
      <ul>{checked.limitations.map(item => <li key={item}>{item}</li>)}</ul>
    </section>}
    {error && <ErrorText text={error} alert className="research-error" />}
    <button type="submit" className="chat-primary-button" disabled={!canSubmit}>{busy && <LoaderCircle size={14} className="chat-spin" aria-hidden="true" />}{checked ? t("researchPreflightStart") : t("researchStart")}</button>
  </form>;
}
