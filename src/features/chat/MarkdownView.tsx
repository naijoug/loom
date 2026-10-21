import { parseMarkdown } from "./markdown";
import { InlineCard } from "./openui/InlineCard";
function inline(text: string) {
  return text.split(/(`[^`]+`)/g).map((part, index) => part.startsWith("`") && part.endsWith("`") ? <code key={index}>{part.slice(1, -1)}</code> : part);
}
/** Present when experimental inline cards are on; `openui-lang` blocks then render as cards. */
export interface MarkdownCards { streaming: boolean; onInsert?: (text: string) => void }
export function Markdown({ text, cards }: { text: string; cards?: MarkdownCards }) {
  return <div className="chat-markdown">{parseMarkdown(text).map((block, index) => block.type === "code"
    ? cards && block.language === "openui-lang" ? <InlineCard key={index} source={block.text} streaming={cards.streaming} onInsert={cards.onInsert} />
    : <pre key={index}><code data-language={block.language}>{block.text}</code></pre>
    : block.type === "list" ? <ul key={index}>{block.text.split("\n").map((line, i) => <li key={i}>{inline(line.replace(/^\s*(?:[-*]|\d+\.)\s/, ""))}</li>)}</ul>
    : <p key={index}>{inline(block.text)}</p>)}</div>;
}
