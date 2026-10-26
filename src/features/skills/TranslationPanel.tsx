import { useEffect, useRef, useState } from "react";
import type { SkillsApi, SkillDetail, TranslationProviderId, TranslationProviders, TranslationResult, TranslationSourceMode, TranslationSummary } from "../../api/skills";
import { useT } from "../../i18n";
import { Markdown } from "../chat/MarkdownView";
import { skillsError } from "./model";
type Api = Pick<SkillsApi, "translationProviders" | "translate" | "translationHistory" | "translationResult" | "cancel">;
export function TranslationPanel({ api, detail, desktop, initialMode = "markdown" }: { api: Api; detail: SkillDetail; desktop: boolean; initialMode?: TranslationSourceMode }) {
  const t = useT();
  const [providers, setProviders] = useState<TranslationProviders | null>(null);
  const [providerId, setProviderId] = useState<TranslationProviderId>("openai");
  const [language, setLanguage] = useState("zh-CN");
  const [mode, setMode] = useState<TranslationSourceMode>(initialMode);
  const [historyLoaded, setHistoryLoaded] = useState(false);
  const [history, setHistory] = useState<TranslationSummary[]>([]);
  const [selected, setSelected] = useState("");
  const [result, setResult] = useState<TranslationResult | null>(null);
  const [error, setError] = useState("");
  const [starting, setStarting] = useState(false);
  const [copied, setCopied] = useState(false);
  const [reload, setReload] = useState(0);
  const alive = useRef(false);
  const startingRef = useRef(false);
  const request = useRef(0);
  const historyRequest = useRef(0);
  useEffect(() => {
    let disposed = false, polling = false; alive.current = true; setHistoryLoaded(false);
    if (!desktop) return () => { alive.current = false; };
    void api.translationProviders().then(value => { if (!disposed) { setProviders(value); setProviderId(value.providers.find(item => item.configured)?.id ?? "openai"); } }, e => { if (!disposed) setError(skillsError(e)); });
    const load = async () => {
      if (disposed || polling) return; polling = true; const generation = ++historyRequest.current;
      try { const next = await api.translationHistory(detail.summary.id); if (!disposed && generation === historyRequest.current) { setHistory(next); setHistoryLoaded(true); setSelected(old => old || next.find(item => item.context.sourceDigest === detail.contentDigest)?.context.operationId || next[0]?.context.operationId || ""); } }
      catch (failure) { if (!disposed) setError(skillsError(failure)); }
      finally { polling = false; }
    };
    void load(); const timer = setInterval(() => { void load(); }, 1500);
    return () => { disposed = true; alive.current = false; clearInterval(timer); };
  }, [api, detail.summary.id, detail.contentDigest, desktop, reload]);
  const chosen = history.find(item => item.context.operationId === selected);
  const phase = chosen?.operation.phase;
  useEffect(() => {
    const seq = ++request.current; setResult(null); setCopied(false);
    if (!selected || phase !== "committed") return;
    void api.translationResult(selected).then(value => {
      if (alive.current && seq === request.current && value.context.operationId === selected && value.context.skillId === detail.summary.id) setResult(value);
    }, failure => { if (alive.current && seq === request.current) setError(skillsError(failure)); });
    return () => { request.current++; };
  }, [api, selected, phase, detail.summary.id]);
  const active = starting || history.some(item => item.operation.phase === "running");
  const provider = providers?.providers.find(item => item.id === providerId);
  const start = async () => {
    if (startingRef.current || active || !historyLoaded || !desktop || !provider?.configured || !language.trim()) return;
    startingRef.current = true; historyRequest.current++; setStarting(true); setError(""); setCopied(false);
    try { const value = await api.translate({ skillId: detail.summary.id, expectedContentDigest: detail.contentDigest, providerId, targetLanguage: language.trim(), sourceMode: mode }); if (alive.current) { historyRequest.current++; setHistory(old => [value, ...old.filter(item => item.context.operationId !== value.context.operationId)]); setSelected(value.context.operationId); } }
    catch (failure) { if (alive.current) setError(skillsError(failure)); }
    finally { startingRef.current = false; if (alive.current) setStarting(false); }
  };
  return <section className="skills-translation-panel" aria-label={t("skillsTranslate")}>
    <p>{t("skillsTranslationSendHint")}</p>
    <div className="skills-install-options">
      <label>{t("skillsTranslationProvider")}<select value={providerId} disabled={active || !providers} onChange={e => setProviderId(e.target.value as TranslationProviderId)}>{providers?.providers.map(item => <option key={item.id} value={item.id}>{item.label}{item.configured ? "" : ` · ${t("skillsTranslationUnavailable")}`}</option>)}</select></label>
      <label>{t("skillsTranslationLanguage")}<input value={language} maxLength={80} disabled={active} onChange={e => setLanguage(e.target.value)} /></label>
      <label>{t("skillsTranslationScope")}<select value={mode} disabled={active} onChange={e => setMode(e.target.value as TranslationSourceMode)}><option value="markdown">{t("skillsTranslationFull")}</option><option value="summary">{t("skillsTranslationSummary")}</option></select></label>
    </div>
    {provider?.model && <p>{t("skillsTranslationModel")}: {provider.model}</p>}
    {!provider?.configured && providers && <p>{t("skillsTranslationConfigureHint")}</p>}
    <div className="skills-install-actions"><button type="button" disabled={!desktop || !historyLoaded || active || !provider?.configured || !language.trim()} onClick={() => { void start(); }}>{t(active ? "skillsWorking" : "skillsTranslate")}</button><button type="button" disabled={active} onClick={() => { setError(""); setReload(v => v + 1); }}>{t("skillsReloadTranslationSettings")}</button></div>
    {error && <p role="alert" className="skills-error">{error}</p>}
    {!!history.length && <label className="skills-translation-history">{t("skillsTranslationHistory")}<select value={selected} onChange={e => setSelected(e.target.value)}>{history.map(item => <option key={item.context.operationId} value={item.context.operationId}>{item.context.providerId} · {item.context.targetLanguage} · {item.context.sourceMode} · {new Date(item.context.createdAtMs).toLocaleString()}</option>)}</select></label>}
    {chosen && <div role="status"><p>{t(phase === "running" ? "skillsWorking" : phase === "committed" ? "skillsDone" : phase === "cancelled" ? "skillsCancelled" : "skillsFailed")}</p>{chosen.operation.error && <p className="skills-error">{chosen.operation.error.message}</p>}{phase === "running" && <button type="button" onClick={() => { void api.cancel(selected).catch(failure => { if (alive.current) setError(skillsError(failure)); }); }}>{t("skillsCancel")}</button>}{chosen.context.sourceDigest !== detail.contentDigest && <p className="skills-notice">{t("skillsTranslationOldVersion")}</p>}</div>}
    {result?.context.operationId === selected && <><button type="button" onClick={() => { void navigator.clipboard.writeText(result.markdown).then(() => { if (alive.current) setCopied(true); }, failure => { if (alive.current) setError(skillsError(failure)); }); }}>{t(copied ? "skillsTranslationCopied" : "skillsTranslationCopy")}</button><Markdown text={result.markdown} /></>}
  </section>;
}
