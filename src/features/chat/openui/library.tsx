import { createLibrary, createParser, defineComponent, type ParseResult, type PromptOptions } from "@openuidev/react-lang";
import { z } from "zod";

// Experimental inline cards (OpenUI Lang). Props are plain text, numbers and booleans only:
// no URLs, HTML, action props, Query() or Mutation(), so a card cannot trigger side effects.
// Required parameters come first because OpenUI arguments are positional and only trailing
// optional arguments may be omitted.

export const FileList = defineComponent({
  name: "FileList",
  description: "Project files with a short note for each. Use when pointing the user at several files.",
  props: z.object({
    files: z.array(z.object({ path: z.string().describe("Project-relative path"), note: z.string().optional() })),
    title: z.string().optional(),
  }),
  component: ({ props }) => <section className="chat-card" data-card="files">
    {props.title && <h4>{props.title}</h4>}
    <ul>{(props.files ?? []).map((file, index) => <li key={index}><code>{file.path}</code>{file.note && <span>{file.note}</span>}</li>)}</ul>
  </section>,
});

export const ChangeSummary = defineComponent({
  name: "ChangeSummary",
  description: "Summary of code changes: per-file added/removed line counts plus key points.",
  props: z.object({
    title: z.string(),
    files: z.array(z.object({ path: z.string(), added: z.number(), removed: z.number() })),
    points: z.array(z.string()).optional(),
  }),
  component: ({ props }) => <section className="chat-card" data-card="changes">
    <h4>{props.title}</h4>
    <ul>{(props.files ?? []).map((file, index) => <li key={index}>
      <code>{file.path}</code><span className="chat-diff-stats"><b>+{file.added}</b><i>−{file.removed}</i></span>
    </li>)}</ul>
    {props.points && props.points.length > 0 && <ul>{props.points.map((point, index) => <li key={index}>{point}</li>)}</ul>}
  </section>,
});

export const DataTable = defineComponent({
  name: "DataTable",
  description: "Plain-text comparison or data table.",
  props: z.object({
    columns: z.array(z.string()),
    rows: z.array(z.array(z.string())),
    title: z.string().optional(),
  }),
  component: ({ props }) => <section className="chat-card" data-card="table">
    {props.title && <h4>{props.title}</h4>}
    <table>
      <thead><tr>{(props.columns ?? []).map((column, index) => <th key={index}>{column}</th>)}</tr></thead>
      <tbody>{(props.rows ?? []).map((row, index) => <tr key={index}>{row.map((cell, cellIndex) => <td key={cellIndex}>{cell}</td>)}</tr>)}</tbody>
    </table>
  </section>,
});

export const Checklist = defineComponent({
  name: "Checklist",
  description: "Steps or verification items with completion state.",
  props: z.object({
    items: z.array(z.object({ text: z.string(), done: z.boolean() })),
    title: z.string().optional(),
  }),
  component: ({ props }) => <section className="chat-card" data-card="checklist">
    {props.title && <h4>{props.title}</h4>}
    <ul>{(props.items ?? []).map((item, index) => <li key={index} data-done={item.done}>{item.text}</li>)}</ul>
  </section>,
});

export const CardStack = defineComponent({
  name: "CardStack",
  description: "Root container holding one or more cards in order.",
  props: z.object({
    cards: z.array(z.union([FileList.ref, ChangeSummary.ref, DataTable.ref, Checklist.ref])),
  }),
  component: ({ props, renderNode }) => <div className="chat-card-stack">{renderNode(props.cards)}</div>,
});

export const inlineCardLibrary = createLibrary({
  components: [CardStack, FileList, ChangeSummary, DataTable, Checklist],
  root: "CardStack",
});

export const inlineCardPromptOptions: PromptOptions = {
  preamble: "Loom Chat can render optional inline cards from OpenUI Lang. Markdown remains the primary answer format.",
  inlineMode: true,
  additionalRules: [
    "Use a card only when a structured view clearly helps: several files, a change summary, a comparison table, or a checklist. Otherwise answer in Markdown only.",
    "Each openui-lang block is a complete, standalone program: always include the root = CardStack([...]) statement and every referenced statement. Never output partial edits to earlier cards.",
    "At most one openui-lang block per reply, placed after the prose explanation. Keep cards small (at most 12 rows or items).",
    "Card text is plain text in the user's language: no Markdown, HTML, or URLs inside card strings.",
  ],
  examples: [
    'root = CardStack([files])\nfiles = FileList([{path: "src/main.tsx", note: "React entry"}, {path: "src-tauri/src/lib.rs", note: "Tauri commands"}], "Entry points")',
  ],
};

/** The system prompt appended for agents that support inline cards; generated into src-tauri/resources. */
export function inlineCardPrompt(): string {
  return inlineCardLibrary.prompt(inlineCardPromptOptions);
}

export interface InlineCardCheck {
  ok: boolean;
  /** Parser error codes plus Loom-specific reasons (`no-root`, `incomplete`, `unresolved`, `empty`). */
  problems: string[];
  cardCount: number;
}

let parser: ReturnType<typeof createParser> | undefined;

/**
 * Decides whether a finished openui-lang block should render as a card. The OpenUI Renderer
 * silently drops invalid components, so fallback to source relies on the parser result instead.
 */
export function checkInlineCard(source: string): InlineCardCheck {
  parser ??= createParser(inlineCardLibrary.toJSONSchema(), "CardStack");
  const result: ParseResult = parser.parse(source);
  const problems: string[] = result.meta.errors.map(error => error.code);
  if (!result.root || result.root.typeName !== "CardStack") problems.push("no-root");
  if (result.meta.incomplete) problems.push("incomplete");
  if (result.meta.unresolved.length) problems.push("unresolved");
  const cards = result.root?.props.cards;
  const cardCount = Array.isArray(cards) ? cards.filter(card => card != null).length : 0;
  if (result.root && !cardCount) problems.push("empty");
  return { ok: problems.length === 0, problems, cardCount };
}

interface CardProps {
  title?: string;
  files?: Array<{ path: string; note?: string; added?: number; removed?: number }>;
  points?: string[];
  columns?: string[];
  rows?: string[][];
  items?: Array<{ text: string; done: boolean }>;
}

function cardText(typeName: string, props: CardProps): string {
  const lines = props.title ? [props.title] : [];
  if (typeName === "FileList") lines.push(...(props.files ?? []).map(file => file.note ? `${file.path} — ${file.note}` : file.path));
  if (typeName === "ChangeSummary") {
    lines.push(...(props.files ?? []).map(file => `${file.path} +${file.added ?? 0} −${file.removed ?? 0}`));
    lines.push(...(props.points ?? []).map(point => `- ${point}`));
  }
  if (typeName === "DataTable") lines.push((props.columns ?? []).join(" | "), ...(props.rows ?? []).map(row => row.join(" | ")));
  if (typeName === "Checklist") lines.push(...(props.items ?? []).map(item => `[${item.done ? "x" : " "}] ${item.text}`));
  return lines.join("\n");
}

/** Plain text of a valid card, for Loom's copy and insert-into-composer controls. */
export function inlineCardText(source: string): string {
  parser ??= createParser(inlineCardLibrary.toJSONSchema(), "CardStack");
  const cards = parser.parse(source).root?.props.cards;
  if (!Array.isArray(cards)) return "";
  return cards
    .filter((card): card is { typeName: string; props: CardProps } => card != null && typeof card === "object" && "typeName" in card)
    .map(card => cardText(card.typeName, card.props))
    .join("\n\n");
}

