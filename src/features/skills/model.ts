import type { SkillsLibrary, SkillsOperation } from "../../api/skills";
import type { MessageKey } from "../../i18n/en";

export function operationActionKey(action: string): MessageKey {
  const keys: Record<string, MessageKey> = { translate: "skillsTranslate", "install-fzf": "skillsInstallFzf", import: "skillsImport", refresh: "skillsRefresh", install: "skillsInstall", uninstall: "skillsUninstall", recover: "skillsRecover" };
  return keys[action] ?? "skillsSourceOperation";
}

export function visibleSkills(library: SkillsLibrary, sourceId: string, query: string) {
  const needle = query.trim().toLocaleLowerCase();
  return library.skills.filter(skill => (!sourceId || skill.sourceId === sourceId)
    && (!needle || [skill.name, skill.title, skill.description, skill.category, skill.keywords, library.sources.find(s => s.id === skill.sourceId)?.name].join(" ").toLocaleLowerCase().includes(needle)));
}
export function mergeOperation(records: SkillsOperation[], incoming: SkillsOperation) {
  const existing = records.find(record => record.operationId === incoming.operationId);
  if (existing && existing.sequence > incoming.sequence) return records;
  return [incoming, ...records.filter(record => record.operationId !== incoming.operationId)].sort((a, b) => b.startedAtMs - a.startedAtMs);
}
export function skillsError(error: unknown): string {
  if (error && typeof error === "object" && "message" in error && typeof error.message === "string") return error.message;
  return String(error);
}

export function skillBody(content: string): string {
  return content.replace(/^---\r?\n[\s\S]*?\r?\n---[ \t]*(?:\r?\n|$)/, "");
}
