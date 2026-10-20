export type MarkdownBlock = { type: "paragraph" | "code" | "list"; text: string; language?: string };
/** Parse a small Markdown subset; rendering uses React text nodes, never HTML. */
export function parseMarkdown(source: string): MarkdownBlock[] {
  const blocks: MarkdownBlock[] = [];
  let lines: string[] = [], code = false, language = "";
  const flush = () => {
    if (lines.length) blocks.push({ type: code ? "code" : lines.every(line => /^\s*(?:[-*]|\d+\.)\s/.test(line)) ? "list" : "paragraph", text: lines.join("\n"), ...(code ? { language } : {}) });
    lines = [];
  };
  for (const line of source.split("\n")) {
    if (line.startsWith("```")) { flush(); code = !code; language = code ? line.slice(3).trim() : ""; }
    else if (!code && !line.trim()) flush();
    else lines.push(line);
  }
  flush(); return blocks;
}
