import { useCallback, useEffect, useRef, useState } from "react";
import type { InstallMode, InstallPlan, InstallReceipt, InstallStatus, SkillsApi } from "../../api/skills";
import type { MessageKey } from "../../i18n/en";
import { useT } from "../../i18n";
import { skillsError } from "./model";

export function installStateKey(state: string): MessageKey {
  const keys: Record<string, MessageKey> = { absent: "skillsNotInstalled", installed: "skillsInstalled", modified: "skillsModified", missing: "skillsMissing", conflict: "skillsConflict", legacy: "skillsLegacy", unavailable: "unavailable", recovery_required: "skillsRecoveryRequired", uninstalled: "skillsUninstalled", skipped: "skillsSkipped", failed: "skillsFailed", cancelled: "skillsCancelled", install: "skillsInstall", replace: "skillsReplace", uninstall: "skillsUninstall", blocked: "skillsBlocked", skip: "skillsSkipped" };
  return keys[state] ?? "unknown";
}

export function PlanPreview({ plan, busy, onApply, onCancel }: { plan: InstallPlan; busy: boolean; onApply: () => void; onCancel: () => void }) {
  const t = useT();
  const actionable = plan.items.some(item => !["blocked", "skip"].includes(item.decision));
  return <section className="skills-plan" aria-label={t("skillsPlan")}><h3>{t("skillsPlan")}</h3>
    <p>{plan.action === "install" ? t(plan.mode === "copy" ? "skillsCopyHint" : "skillsLinkHint") : t("skillsUninstallHint")}</p>
    <ul>{plan.items.map(item => <li key={`${item.target.id}/${item.installName}`}><strong>{item.target.label} · {t(item.target.scope === "global" ? "skillsGlobal" : "skillsProject")}</strong><span> · {t(installStateKey(item.decision))}</span><code>{item.destination}</code>{item.message && <p>{item.message}</p>}{item.resourceChanges?.some(r => r.path.endsWith(".md")) && <div><span>{t("skillsManagedFiles")}</span>{item.resourceChanges.filter(r => r.path.endsWith(".md")).map(r => <code key={r.path}>{r.path}</code>)}</div>}</li>)}</ul>
    <div className="skills-install-actions"><button type="button" disabled={busy || !actionable} onClick={onApply}>{t("skillsApplyPlan")}</button><button type="button" disabled={busy} onClick={onCancel}>{t("cancel")}</button></div>
  </section>;
}

export function InstallResults({ receipt }: { receipt: InstallReceipt }) {
  const t = useT();
  return <section className="skills-install-results" aria-label={t("skillsInstallResults")} role="status"><h3>{t("skillsInstallResults")}</h3>{receipt.items.map(item => <div key={`${item.targetId}/${item.destination}`}><strong>{t(installStateKey(item.status))}</strong><code>{item.destination}</code>{item.message && <p>{item.message}</p>}{item.backupDirectory && <p>{t("skillsBackupLocation")}<code>{item.backupDirectory}</code></p>}</div>)}</section>;
}

export function InstallPanel({ api, skillId, projectPath, revision = 0 }: { api: SkillsApi; skillId: string; projectPath: string | null; revision?: number }) {
  const t = useT();
  const [statuses, setStatuses] = useState<InstallStatus[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const [mode, setMode] = useState<InstallMode>("copy");
  const [slash, setSlash] = useState(false);
  const [policy, setPolicy] = useState<"fail" | "skip" | "overwrite">("fail");
  const [plan, setPlan] = useState<InstallPlan | null>(null);
  const [receipt, setReceipt] = useState<InstallReceipt | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [readError, setReadError] = useState("");
  const scope = useRef(0); const read = useRef(0); const actionPending = useRef(false);
  const reads = useRef<Promise<void>>(Promise.resolve());
  const resetSelection = useRef(false);
  const load = useCallback((reset = false): Promise<void> => {
    const request = ++read.current, captured = scope.current;
    resetSelection.current ||= reset;
    const task = reads.current.then(async () => {
      if (captured !== scope.current) return;
      try {
        const next = await api.installStatus(skillId, projectPath);
        if (request !== read.current || captured !== scope.current) return;
        setStatuses(next); setReadError("");
        if (resetSelection.current) {
          resetSelection.current = false;
          const installed = next.filter(item => item.state === "installed");
          setSelected(installed.map(item => item.target.id));
          setSlash(installed.some(item => item.installation?.managedResources.some(resource => resource.marker.startsWith("slash:"))));
          if (installed.length && installed.every(item => item.installation?.mode === installed[0].installation?.mode)) setMode(installed[0].installation!.mode);
        }
      } catch (failure) { if (request === read.current && captured === scope.current) setReadError(skillsError(failure)); }
      finally { if (request === read.current && captured === scope.current) setLoading(false); }
    });
    reads.current = task.catch(() => {});
    return task;
  }, [api, skillId, projectPath]);
  useEffect(() => {
    scope.current++; setPlan(null); setReceipt(null); setSelected([]); setError(""); setReadError(""); setBusy(false); setLoading(true); setMode("copy"); setPolicy("fail"); setSlash(false); actionPending.current = false;
    void load(true);
    return () => { scope.current++; read.current++; };
  }, [load]);
  const lastRevision = useRef(revision);
  useEffect(() => { if (lastRevision.current !== revision) { lastRevision.current = revision; void load(); } }, [revision, load]);
  async function action(work: () => Promise<void>) {
    if (actionPending.current) return;
    const captured = scope.current; actionPending.current = true; setBusy(true); setError("");
    try { await work(); } catch (failure) { if (captured === scope.current) setError(skillsError(failure)); }
    finally { if (captured === scope.current) { setBusy(false); actionPending.current = false; } }
  }
  async function preview(uninstall: boolean) {
    const captured = scope.current;
    await action(async () => {
      const next = uninstall
        ? await api.planUninstall(statuses.filter(item => selected.includes(item.target.id) && item.installation).map(item => item.installation!.installationId), projectPath)
        : await api.planInstall({ skillId, projectPath, targetIds: selected, mode, conflictPolicy: policy, withSlashCommands: slash });
      if (captured === scope.current) { setPlan(next); setReceipt(null); }
    });
  }
  async function apply() {
    if (!plan) return;
    const captured = scope.current, submitted = plan;
    await action(async () => {
      const next = await api.applyInstall(submitted.planId, submitted.projectPath);
      if (captured === scope.current) { setReceipt(next); setPlan(null); await load(); }
    });
  }
  const canUninstall = statuses.some(item => selected.includes(item.target.id) && item.installation);
  return <section className="skills-install-panel" aria-label={t("skillsInstallToTools")} aria-busy={busy || loading}>
    <div className="skills-section-heading"><h3>{t("skillsInstallToTools")}</h3><button type="button" disabled={busy || loading} onClick={() => { void load(); }}>{t("skillsReload")}</button></div><p>{t("skillsInstallBoundary")}</p><p>{t("skillsFileOnlyHint")}</p><p>{t("skillsSharedPathHint")}</p>
    {readError && <p role="alert" className="skills-error">{readError}</p>}
    {error && <p role="alert" className="skills-error">{error}</p>}
    {loading ? <p role="status">{t("loading")}</p> : <>
      <div className="skills-install-options"><label>{t("skillsInstallMode")}<select value={mode} disabled={busy} onChange={event => { setMode(event.target.value as InstallMode); setPlan(null); }}><option value="copy">{t("skillsCopy")}</option><option value="symlink">{t("skillsSymlink")}</option></select></label>
        <label>{t("skillsConflictPolicy")}<select value={policy} disabled={busy} onChange={event => { setPolicy(event.target.value as typeof policy); setPlan(null); }}><option value="fail">{t("skillsKeepExisting")}</option><option value="skip">{t("skillsSkipExisting")}</option><option value="overwrite">{t("skillsReplaceManaged")}</option></select></label></div>
      <label className="skills-wrapper-option"><input type="checkbox" checked={slash} disabled={busy} onChange={event => { setSlash(event.target.checked); setPlan(null); }} />{t("skillsSlashOption")}</label>
      <div className="skills-install-targets">{statuses.map(item => <label key={item.target.id}><input type="checkbox" checked={selected.includes(item.target.id)} disabled={busy || !!item.target.blockedReason || ["legacy", "unavailable", "recovery_required"].includes(item.state)} onChange={event => { setSelected(previous => event.target.checked ? [...previous, item.target.id] : previous.filter(id => id !== item.target.id)); setPlan(null); }} />
        <span><strong>{item.target.label} · {t(item.target.scope === "global" ? "skillsGlobal" : "skillsProject")}</strong><code>{item.destination}</code><small>{t(installStateKey(item.state))}{item.updateAvailable && ` · ${t("skillsUpdateAvailable")}`}</small>{item.message && <small>{item.message}</small>}<InstructionChecks item={item} /></span></label>)}</div>
      <div className="skills-install-actions"><button type="button" disabled={busy || !selected.length} onClick={() => { void preview(false); }}>{t("skillsPreviewInstall")}</button><button type="button" disabled={busy || !canUninstall} onClick={() => { void preview(true); }}>{t("skillsPreviewUninstall")}</button></div>
    </>}
    {plan && <PlanPreview plan={plan} busy={busy} onApply={() => { void apply(); }} onCancel={() => setPlan(null)} />}
    {receipt && <InstallResults receipt={receipt} />}
  </section>;
}

export function InstructionChecks({ item }: { item: InstallStatus }) {
  const t = useT();
  const labels = { current: "skillsInstructionCurrent", missing: "skillsInstructionMissing", stale: "skillsInstructionStale", malformed: "skillsInstructionMalformed" } as const;
  return <>{item.instructionChecks?.map(check => <small key={check.path}>{t(labels[check.state])}<code>{check.path}</code>{check.message && <span>{check.message}</span>}</small>)}</>;
}
