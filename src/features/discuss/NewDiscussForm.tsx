import { useState, type FormEvent } from "react";
import { MessagesSquare, Plus, X } from "lucide-react";
import { DISCUSS_ADAPTERS, type DiscussApi, type CreateDiscussionInput } from "../../api/discuss";
import type { AgentConfig, AgentDiagnostic, ProjectSummary } from "../../domain";
import { AgentPicker, agentSupportsModel, isAgentUnusable } from "../../components/AgentPicker";
import { ErrorText } from "../../components/common/ErrorText";
import { useLocale, useT } from "../../i18n";
export function discussAgents(agents: AgentConfig[]) {
  return agents.filter((a) => a.enabled && DISCUSS_ADAPTERS.includes(a.adapterType));
}
export function NewDiscussForm({
  api,
  agents,
  diagnostics,
  projects,
  onCreated,
}: {
  api: DiscussApi;
  agents: AgentConfig[];
  diagnostics: AgentDiagnostic[];
  projects: ProjectSummary[];
  onCreated: (id: string) => void;
}) {
  const t = useT(),
    { locale } = useLocale();
  const eligible = discussAgents(agents);
  const [topic, setTopic] = useState("");
  const [participants, setParticipants] = useState(() =>
    [0, 1].map((i) => ({ agentId: eligible[i]?.id ?? "", model: "" })),
  );
  const [projectPath, setProjectPath] = useState("");
  const [language, setLanguage] = useState<"en" | "zh-CN">(locale === "zh-CN" ? "zh-CN" : "en");
  const [confirmation, setConfirmation] = useState<CreateDiscussionInput | null>(null);
  const [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const choices = participants.map((p) => ({
    agentId: p.agentId,
    model: agentSupportsModel(diagnostics, p.agentId) ? p.model.trim() || undefined : undefined,
  }));
  const duplicate = new Set(choices.map((p) => JSON.stringify(p))).size !== choices.length;
  const valid =
    topic.trim().length > 0 &&
    !duplicate &&
    choices.every(
      (p) =>
        eligible.some((a) => a.id === p.agentId && a.available) &&
        !isAgentUnusable(diagnostics.find((d) => d.agentId === p.agentId)?.status) &&
        (!p.model || (!/\s/.test(p.model) && !p.model.startsWith("-") && p.model.length <= 128)),
    );
  const update = (index: number, patch: Partial<(typeof participants)[number]>) => {
    setConfirmation(null);
    setParticipants((ps) => ps.map((p, i) => (i === index ? { ...p, ...patch } : p)));
  };
  function submit(event: FormEvent) {
    event.preventDefault();
    if (valid && !busy)
      setConfirmation({
        topic: topic.trim(),
        participants: choices,
        projectPath: projectPath || undefined,
        language,
        confirmedCalls: choices.length * 2 + 1,
      });
  }
  async function start() {
    if (!confirmation || busy) return;
    setBusy(true);
    setError("");
    try {
      onCreated((await api.create(confirmation)).discussionId);
    } catch (reason) {
      setError(String(reason));
      setConfirmation(null);
    } finally {
      setBusy(false);
    }
  }
  return (
    <form className="discuss-new" onSubmit={submit} aria-labelledby="discuss-new-title">
      <span className="chat-empty-mark">
        <MessagesSquare size={24} />
      </span>
      <h1 id="discuss-new-title">{t("discussNewTitle")}</h1>
      <p className="discuss-note">{t("discussNewDesc")}</p>
      <fieldset disabled={busy || !!confirmation}>
        <label>
          {t("discussTopic")}
          <textarea
            autoFocus
            required
            rows={3}
            maxLength={8000}
            value={topic}
            onChange={(e) => setTopic(e.target.value)}
            placeholder={t("discussTopicPlaceholder")}
          />
        </label>
        <div className="discuss-participants">
          {participants.map((p, i) => (
            <div
              key={i}
              className="discuss-participant"
              role="group"
              aria-label={t("discussParticipant", { index: i + 1 })}
            >
              <div className="discuss-participant-label">
                <span>
                  {t("discussParticipant", { index: i + 1 })}
                  {i === 0 && ` · ${t("discussModerator")}`}
                </span>
                <button
                  type="button"
                  className="chat-icon-button"
                  aria-label={t("discussRemoveParticipant", { index: i + 1 })}
                  disabled={participants.length <= 2}
                  onClick={() => setParticipants((ps) => ps.filter((_, n) => i !== n))}
                >
                  <X size={14} />
                </button>
              </div>
              <AgentPicker
                agents={eligible}
                diagnostics={diagnostics}
                value={p.agentId}
                onChange={(agentId) => update(i, { agentId, model: "" })}
                placement="down"
                disabled={busy || !!confirmation}
                model={{ value: p.model, onChange: (model) => update(i, { model }) }}
              />
            </div>
          ))}
        </div>
        <button
          type="button"
          className="discuss-add"
          disabled={participants.length >= 4}
          onClick={() =>
            setParticipants((ps) => [
              ...ps,
              {
                agentId: eligible.find((a) => !ps.some((p) => p.agentId === a.id))?.id ?? eligible[0]?.id ?? "",
                model: "",
              },
            ])
          }
        >
          <Plus size={14} />
          {t("discussAddParticipant")}
        </button>
        {duplicate && <p className="discuss-note">{t("discussDuplicate")}</p>}
        <div className="discuss-form-row">
          <label>
            {t("discussProject")}
            <select value={projectPath} onChange={(e) => setProjectPath(e.target.value)}>
              <option value="">{t("discussNoProject")}</option>
              {projects.map((p) => (
                <option key={p.path} value={p.path}>
                  {p.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            {t("discussLanguage")}
            <select value={language} onChange={(e) => setLanguage(e.target.value as typeof language)}>
              <option value="en">English</option>
              <option value="zh-CN">中文</option>
            </select>
          </label>
        </div>
      </fieldset>
      <p className="discuss-note">{t("discussCalls", { count: participants.length * 2 + 1 })}</p>
      {!confirmation && (
        <button type="submit" className="chat-primary-button" disabled={!valid || busy}>
          {t("discussReviewStart")}
        </button>
      )}
      {confirmation && (
        <div className="discuss-banner" role="alertdialog" aria-label={t("discussConfirmTitle")}>
          <span>{t("discussConfirmCalls", { count: confirmation.confirmedCalls })}</span>
          <button type="button" disabled={busy} onClick={() => setConfirmation(null)}>
            {t("cancel")}
          </button>
          <button type="button" className="chat-primary-button" disabled={busy} onClick={() => void start()}>
            {t("discussConfirmStart")}
          </button>
        </div>
      )}
      {error && <ErrorText text={error} alert className="discuss-error" />}
    </form>
  );
}
