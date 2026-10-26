import { useEffect, useRef, useState } from "react";
import type { SkillDetail } from "../../api/skills";
import { useT } from "../../i18n";

function fenced(text: string, language = "") {
  const longest = Math.max(2, ...Array.from(text.matchAll(/`+/g), match => match[0].length));
  const fence = "`".repeat(longest + 1);
  return `${fence}${language}\n${text.trim()}\n${fence}`;
}
export function shareText(detail: SkillDetail, sourceName: string): string {
  const sections = [`# ${detail.summary.title}`, `Source: ${sourceName} / ${detail.summary.relativePath}`, "## SKILL.md", fenced(detail.content, "markdown")];
  if (detail.relatedFiles.length) {
    sections.push("## Related files");
    for (const file of detail.relatedFiles) sections.push(`### ${file.relativePath}`, file.content === null ? `_No text preview available; ${file.sizeBytes} bytes._` : fenced(file.content));
  }
  return `${sections.join("\n\n")}\n`;
}
export function SkillActions({ detail, sourceName, onSearch, onSources, onInstall, onTranslate, onSettings }: {
  detail: SkillDetail | null; sourceName: string;
  onSearch(): void; onSources(): void; onInstall(): void; onTranslate(): void; onSettings(): void;
}) {
  const t = useT();
  const [open, setOpen] = useState(false), [query, setQuery] = useState("");
  const [index, setIndex] = useState(0), [status, setStatus] = useState("");
  const [fallback, setFallback] = useState("");
  const generation = useRef(0), input = useRef<HTMLInputElement>(null), trigger = useRef<HTMLButtonElement>(null);
  useEffect(() => { generation.current++; setStatus(""); setFallback(""); return () => { generation.current++; }; }, [detail?.summary.id, detail?.contentDigest]);
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") { event.preventDefault(); setOpen(true); }
    };
    window.addEventListener("keydown", key); return () => window.removeEventListener("keydown", key);
  }, []);
  useEffect(() => { if (open) input.current?.focus(); }, [open]);
  const copy = async (text: string) => {
    const current = ++generation.current; setStatus(""); setFallback("");
    try { await navigator.clipboard.writeText(text); if (generation.current === current) setStatus(t("skillsTranslationCopied")); }
    catch { if (generation.current === current) { setStatus(t("skillsCopyFallback")); setFallback(text); } }
  };
  const commands = [
    { id: "search", title: t("skillsSearchCommand"), keywords: "search find filter 搜索", run: onSearch },
    { id: "sources", title: t("skillsAddSource"), keywords: "repo repository import refresh source 来源 仓库", run: onSources },
    { id: "install", title: t("skillsInstallTab"), keywords: "install target manage 安装", disabled: !detail, run: onInstall },
    { id: "path", title: t("skillsCopyPath"), keywords: "copy relative path clipboard 路径", disabled: !detail, run: () => { if (detail) void copy(detail.summary.relativePath); } },
    { id: "markdown", title: t("skillsCopyMarkdown"), keywords: "copy markdown content 正文", disabled: !detail, run: () => { if (detail) void copy(detail.content); } },
    { id: "translate", title: t("skillsTranslateSummary"), keywords: "translate translation language summary 翻译", disabled: !detail, run: onTranslate },
    { id: "share", title: t("skillsCopyShare"), keywords: "export gist bundle share 分享 导出", disabled: !detail, run: () => { if (detail) void copy(shareText(detail, sourceName)); } },
    { id: "settings", title: t("skillsTranslationSettings"), keywords: "settings preferences prefs 设置", run: onSettings },
  ].filter(command => `${command.title} ${command.keywords}`.toLowerCase().includes(query.trim().replace(/^>\s*/, "").toLowerCase()));
  const close = () => { setOpen(false); setQuery(""); setIndex(0); trigger.current?.focus(); };
  const run = (at: number) => { const command = commands[at]; if (!command || command.disabled) return; close(); command.run(); };
  return <section className="skills-actions" aria-label={t("skillsCommands")}>
    <button ref={trigger} type="button" aria-expanded={open} onClick={() => setOpen(value => !value)}>{t("skillsCommands")} <kbd>⌘/Ctrl K</kbd></button>
    {detail && <><button type="button" onClick={() => { void copy(detail.content); }}>{t("skillsCopyMarkdown")}</button><button type="button" onClick={() => { void copy(shareText(detail, sourceName)); }}>{t("skillsCopyShare")}</button></>}
    {open && <div className="skills-command-list"><input ref={input} role="combobox" aria-label={t("skillsCommands")} aria-expanded="true" aria-controls="skills-command-options" aria-activedescendant={commands[index] ? `skills-command-${commands[index].id}` : undefined} value={query} onChange={event => { setQuery(event.target.value); setIndex(0); }} onKeyDown={event => {
      if (event.nativeEvent.isComposing) return;
      if (event.key === "Escape") { event.preventDefault(); close(); }
      else if (commands.length && ["ArrowDown", "ArrowUp"].includes(event.key)) { event.preventDefault(); setIndex(value => (value + (event.key === "ArrowDown" ? 1 : -1) + commands.length) % commands.length); }
      else if (event.key === "Enter") { event.preventDefault(); run(index); }
    }} /><div id="skills-command-options" role="listbox">{commands.map((command, at) => <button id={`skills-command-${command.id}`} type="button" role="option" key={command.id} aria-selected={index === at} disabled={command.disabled} onClick={() => run(at)}>{command.title}</button>)}{!commands.length && <p>{t("skillsNoMatches")}</p>}</div></div>}
    {status && <p role="status">{status}</p>}
    {fallback && <textarea aria-label={t("skillsShareText")} readOnly value={fallback} onFocus={event => event.target.select()} />}
  </section>;
}
