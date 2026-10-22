import { useEffect, useState } from "react";
import type { SkillsApi, TranslationProvider, TranslationProviders, TranslationConfigInput } from "../../api/skills";
import { useT } from "../../i18n";
import { skillsError } from "./model";

type Api = Pick<SkillsApi, "translationProviders" | "saveTranslationConfig">;
export function TranslationSettings({ api }: { api: Api }) {
  const t = useT();
  const [data, setData] = useState<TranslationProviders | null>(null);
  const [error, setError] = useState("");
  const [reload, setReload] = useState(0);
  const [saving, setSaving] = useState(false);
  const [saved, setSaved] = useState(false);
  useEffect(() => {
    let disposed = false;
    setData(null); setError("");
    void api.translationProviders().then(value => { if (!disposed) setData(value); }, failure => { if (!disposed) setError(skillsError(failure)); });
    return () => { disposed = true; };
  }, [api, reload]);
  return <section className="skills-translation-settings" aria-label={t("skillsTranslationSettings")}>
    <h2>{t("skillsTranslationSettings")}</h2><p>{t("skillsTranslationConfigHint")}</p>
    <button type="button" disabled={saving} onClick={() => setReload(value => value + 1)}>{t("skillsReloadTranslationSettings")}</button>
    {error && <p role="alert" className="skills-error">{error}</p>}
    {!data && !error && <p role="status">{t("loading")}</p>}
    {saved && <p role="status">{t("skillsConfigSaved")}</p>}
    {data && <div className="skills-provider-grid">{data.providers.map(provider => <ProviderForm key={provider.id} provider={provider} revision={data.revision} api={api} busy={saving} onBusy={value => { setSaving(value); if (value) setSaved(false); }} onSaved={value => { setData(value); setSaved(true); }} />)}</div>}
  </section>;
}
function ProviderForm({ provider, revision, api, busy, onBusy, onSaved }: { provider: TranslationProvider; revision: number; api: Api; busy: boolean; onBusy: (value: boolean) => void; onSaved: (value: TranslationProviders) => void }) {
  const t = useT();
  const [model, setModel] = useState(provider.storedModel ?? "");
  useEffect(() => setModel(provider.storedModel ?? ""), [provider.storedModel]);
  const [key, setKey] = useState("");
  const [keyAction, setKeyAction] = useState<"keep" | "set" | "clear">("keep");
  const [error, setError] = useState("");
  // A closed panel never writes a late response into a different panel instance.
  const [lifetime] = useState(() => ({ mounted: true }));
  useEffect(() => { lifetime.mounted = true; return () => { lifetime.mounted = false; }; }, [lifetime]);
  const save = async () => {
    if (busy) return;
    onBusy(true); setError("");
    const change: TranslationConfigInput["key"] = keyAction === "set" ? { action: "set", value: key } : { action: keyAction };
    try { const next = await api.saveTranslationConfig({ providerId: provider.id, expectedRevision: revision, key: change, model: model.trim() || null }); if (lifetime.mounted) { setKey(""); setKeyAction("keep"); setModel(next.providers.find(item => item.id === provider.id)?.storedModel ?? ""); onSaved(next); } }
    catch (failure) { if (lifetime.mounted) { setError(skillsError(failure)); setKey(""); setKeyAction("keep"); } }
    finally { if (lifetime.mounted) onBusy(false); }
  };
  return <form className="skills-provider" aria-label={provider.label} onSubmit={event => { event.preventDefault(); void save(); }}>
    <h3>{provider.label}</h3>
    <p>{t(provider.keySource === "environment" ? "skillsKeyEnvironment" : provider.keySource === "stored" ? "skillsKeyStored" : provider.keySource === "local_cli" ? provider.configured ? "skillsLocalCliFound" : "skillsLocalCliMissing" : "skillsKeyMissing")}</p>
    <label>{t("skillsTranslationModel")}<input value={model} disabled={busy} placeholder={provider.model ?? t("skillsProviderDefault")} maxLength={128} onChange={event => setModel(event.target.value)} /></label>
    {provider.modelFromEnvironment && <small>{t("skillsModelEnvironment", { model: provider.model ?? "" })}</small>}
    {provider.supportsApiKey && <>
      <label>{t("skillsTranslationKeyChange")}<select value={keyAction} disabled={busy} onChange={event => { setKeyAction(event.target.value as typeof keyAction); setKey(""); }}><option value="keep">{t("skillsKeyKeep")}</option><option value="set">{t("skillsKeyReplace")}</option><option value="clear">{t("skillsKeyClear")}</option></select></label>
      {keyAction === "set" && <label>{t("skillsTranslationKey")}<input type="password" value={key} disabled={busy} autoComplete="new-password" spellCheck={false} maxLength={4096} onChange={event => setKey(event.target.value)} /></label>}
      {keyAction === "clear" && provider.keySource === "environment" && <small>{t("skillsKeyClearEnvironment")}</small>}
    </>}
    {error && <p role="alert" className="skills-error">{error}</p>}
    <button type="submit" disabled={busy || (keyAction === "set" && !key.trim())}>{t(busy ? "skillsWorking" : "save")}</button>
  </form>;
}
