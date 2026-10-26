import { useEffect, useRef, useState } from "react";
import type { MigrationPreview, SkillsApi } from "../../api/skills";
import { useT } from "../../i18n";
import { skillsError } from "./model";

export function MigrationPanel({ api, projectPath, desktop }: { api: Pick<SkillsApi, "planMigration">; projectPath: string | null; desktop: boolean }) {
  const t = useT();
  const stateLabel = (state: string) => {
    const labels = { read:"skillsMigrationRead", missing:"skillsMigrationMissing", unreadable:"skillsMigrationUnreadable", directory_unverified:"skillsMigrationDirectory", outside_scope_unchecked:"skillsMigrationOutside", linked_or_unreadable:"skillsMigrationLinked", invalid_path:"skillsMigrationInvalid", not_a_directory:"skillsMigrationInvalid", not_recorded:"skillsMigrationNotRecorded", symlink_unverified:"skillsMigrationSymlink", copy_directory_unverified:"skillsMigrationCopy", unexpected_file:"skillsMigrationInvalid", pin_source_after_ownership_review:"skillsMigrationPin", compare_copy_before_adoption:"skillsMigrationCompare", resolve_source_before_adoption:"skillsMigrationResolve", preserve_bundle_source_before_adoption:"skillsMigrationPreserveBundle" } as const;
    return t(labels[state as keyof typeof labels] ?? "skillsLegacyUnverified");
  };
  const [directories, setDirectories] = useState(""), [catalog, setCatalog] = useState("");
  const [includeGlobal, setIncludeGlobal] = useState(false);
  const [preview, setPreview] = useState<MigrationPreview | null>(null);
  const [busy, setBusy] = useState(false), [error, setError] = useState("");
  const generation = useRef(0), pending = useRef(false);
  useEffect(() => { generation.current++; pending.current = false; setPreview(null); setError(""); setBusy(false); return () => { generation.current++; }; }, [api, projectPath]);
  const scan = async () => {
    if (pending.current || !desktop) return;
    const id = ++generation.current; pending.current = true; setBusy(true); setError(""); setPreview(null);
    try {
      const next = await api.planMigration({ dataDirs: directories.split("\n").map(p => p.trim()).filter(Boolean), catalogDir: catalog.trim() || null, projectPath, includeGlobal });
      if (id === generation.current) setPreview(next);
    } catch (failure) { if (id === generation.current) setError(skillsError(failure)); }
    finally { if (id === generation.current) { pending.current = false; setBusy(false); } }
  };
  return <section className="skills-migration" aria-label={t("skillsMigration")} aria-busy={busy}>
    <h2>{t("skillsMigration")}</h2><p>{t("skillsMigrationHint")}</p>
    <p>{t("skillsMigrationScope")}{projectPath && <code>{projectPath}</code>}</p>
    <label>{t("skillsLegacyDataDirs")}<textarea rows={2} value={directories} disabled={busy} placeholder={t("skillsLegacyDefault")} onChange={e => { setDirectories(e.target.value); setPreview(null); }} /></label>
    <label>{t("skillsLegacyCatalog")}<input value={catalog} disabled={busy} placeholder="/path/to/old/skills" onChange={e => { setCatalog(e.target.value); setPreview(null); }} /></label>
    <label className="skills-migration-global"><input type="checkbox" checked={includeGlobal} disabled={busy} onChange={e => { setIncludeGlobal(e.target.checked); setPreview(null); }} />{t("skillsLegacyIncludeGlobal")}</label>
    <button type="button" disabled={!desktop || busy} onClick={() => { void scan(); }}>{t(busy ? "skillsWorking" : "skillsPreviewMigration")}</button>
    {error && <p role="alert" className="skills-error">{error}</p>}
    {preview && <div className="skills-migration-report">
      <p role="status">{t(preview.complete ? "skillsMigrationScanned" : "skillsMigrationReview")}</p>
      <p className="skills-notice">{t("skillsMigrationNoApply")}</p>
      {preview.dataDirectories.map(path => <code key={path}>{path}</code>)}
      {preview.catalogDirectory && <code>{preview.catalogDirectory}</code>}
      <h3>{t("skillsSources")} ({preview.sources.length})</h3>
      {preview.sources.map((source, index) => <article key={`${source.sourceId}:${index}`}><strong>{source.location}</strong><p>{source.kind} · {source.alreadyRegistered ? t("skillsAlreadyRegistered") : t("skillsLegacyUnverified")}</p><p>{t("skillsLegacyAliases")}: {source.legacyIds.join(", ")}</p>{source.cachePath && <code>{source.cachePath}</code>}<small>{stateLabel(source.cacheState)}</small></article>)}
      <h3>{t("skillsManagedInstallations")} ({preview.installations.length})</h3>
      {preview.installations.map((item, index) => <article key={index}><strong>{item.name} · {item.mode}</strong><p>{item.targetId} · {t("skillsLegacyUnverified")}</p><code>{item.sourcePath}</code><p>{stateLabel(item.destinationState)} · {stateLabel(item.sourceState)}</p>{item.linkTarget && <code>{item.linkTarget}</code>}<small>{stateLabel(item.nextStep)}</small></article>)}
      <h3>{t("skillsTranslationSettings")} ({preview.configs.length})</h3>
      {preview.configs.map((config, index) => <article key={index}><strong>{config.provider}</strong><p>{t(config.keyPresent ? "skillsLegacyKeyPresent" : "skillsLegacyKeyAbsent")}</p><p>{t(config.modelPresent ? "skillsLegacyModelPresent" : "skillsLegacyModelAbsent")}</p><code>{config.path}</code></article>)}
      {!!preview.issues.length && <section aria-label={t("skillsMigrationReview")}><h3>{t("skillsMigrationReview")}</h3>{preview.issues.map((issue, index) => <article key={index}><p>{issue.message}</p><code>{issue.path}</code></article>)}</section>}
      <details><summary>{t("skillsMigrationObservations")} ({preview.files.length})</summary><code>{preview.previewId}</code>{preview.files.map((file, index) => <p key={index}><code>{file.path}</code>{stateLabel(file.state)}</p>)}</details>
    </div>}
  </section>;
}
