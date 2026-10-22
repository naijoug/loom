import { useCallback, useEffect, useRef, useState } from "react";
import type { InstallPlan, InstallReceipt, InstallStatus, RecoveryItem, SkillsApi } from "../../api/skills";
import { useT } from "../../i18n";
import { InstallResults, PlanPreview, installStateKey, InstructionChecks } from "./InstallPanel";
import { skillsError } from "./model";

export function InstalledSkills({ api, projectPath, revision }: { api: SkillsApi; projectPath: string | null; revision: number }) {
  const t = useT(); const [entries, setEntries] = useState<InstallStatus[]>([]); const [pending, setPending] = useState<RecoveryItem[]>([]);
  const [error, setError] = useState(""); const [busy, setBusy] = useState(false); const [plan, setPlan] = useState<InstallPlan | null>(null); const [receipt, setReceipt] = useState<InstallReceipt | null>(null);
  const [loading, setLoading] = useState(true);
  const [readError, setReadError] = useState("");
  const reads = useRef<Promise<void>>(Promise.resolve());
  const epoch = useRef(0); const request = useRef(0); const actionPending = useRef(false);
  const load = useCallback((): Promise<void> => {
    const id = ++request.current, scope = epoch.current;
    const task = reads.current.then(async () => {
      if (scope !== epoch.current) return;
      try {
        // Both endpoints use the same coordination domain. Avoid racing them.
        const operations = await api.pending(projectPath);
        const installed = await api.installations(projectPath);
        if (id === request.current && scope === epoch.current) { setEntries(installed); setPending(operations); setReadError(""); }
      } catch (failure) { if (id === request.current && scope === epoch.current) setReadError(skillsError(failure)); }
      finally { if (id === request.current && scope === epoch.current) setLoading(false); }
    });
    reads.current = task.catch(() => {});
    return task;
  }, [api, projectPath]);
  useEffect(() => { epoch.current++; setPlan(null); setReceipt(null); setError(""); setReadError(""); setBusy(false); setLoading(true); actionPending.current = false; void load(); return () => { epoch.current++; request.current++; }; }, [load]);
  const previous = useRef(revision);
  useEffect(() => { if (previous.current !== revision) { previous.current = revision; void load(); } }, [revision, load]);
  async function run(work: (scope: number) => Promise<void>) {
    if (actionPending.current) return;
    const scope = epoch.current; actionPending.current = true; setBusy(true); setError("");
    try { await work(scope); } catch (failure) { if (scope === epoch.current) setError(skillsError(failure)); }
    finally { if (scope === epoch.current) { setBusy(false); actionPending.current = false; } }
  }
  return <section className="skills-installed" aria-label={t("skillsManagedInstallations")}>
    <div className="skills-section-heading"><div><h3>{t("skillsManagedInstallations")}</h3><p>{t("skillsInventoryHint")}</p></div><button type="button" disabled={busy} onClick={() => { void load(); }}>{t("skillsReload")}</button></div>
    {readError && <p className="skills-error" role="alert">{readError}</p>}
    {error && <p className="skills-error" role="alert">{error}</p>}
    {pending.map(item => <section className="skills-plan" key={item.operationId}><h3>{t("skillsRecoveryRequired")}</h3>{item.resources.map(path => <code key={path}>{path}</code>)}{item.message && <p>{item.message}</p>}<button type="button" disabled={busy || !item.canRecover} onClick={() => { void run(async scope => { await api.recover(item.operationId, projectPath); if (scope === epoch.current) await load(); }); }}>{t("skillsRecover")}</button></section>)}
    {loading ? <p role="status">{t("loading")}</p> : !entries.length && <p>{t("skillsNoInstallations")}</p>}
    {entries.map(item => <div className="skills-installed-row" key={item.installation?.installationId ?? item.target.id}><div><strong>{item.installation?.installName ?? item.target.label}</strong><span>{item.target.label} · {t(item.target.scope === "global" ? "skillsGlobal" : "skillsProject")}</span><code>{item.destination}</code><small>{t(installStateKey(item.state))}{item.installation && ` · ${t(item.installation.mode === "copy" ? "skillsCopy" : "skillsSymlink")}`}</small>{item.message && <p>{item.message}</p>}<InstructionChecks item={item} /></div>
      {item.installation && <button type="button" disabled={busy} onClick={() => { void run(async scope => { const next = await api.planUninstall([item.installation!.installationId], projectPath); if (scope === epoch.current) { setPlan(next); setReceipt(null); } }); }}>{t("skillsPreviewUninstall")}</button>}</div>)}
    {plan && <PlanPreview plan={plan} busy={busy} onCancel={() => setPlan(null)} onApply={() => { const captured = plan; void run(async scope => { const result = await api.applyInstall(captured.planId, captured.projectPath); if (scope === epoch.current) { setReceipt(result); setPlan(null); await load(); } }); }} />}
    {receipt && <InstallResults receipt={receipt} />}
  </section>;
}
