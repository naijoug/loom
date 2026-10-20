import { parseMarkdown } from "./markdown";
function inline(text: string) {
  return text.split(/(`[^`]+`)/g).map((part, index) => part.startsWith("`") && part.endsWith("`") ? <code key={index}>{part.slice(1, -1)}</code> : part);
}
export function Markdown({ text }: { text: string }) {
  return <div className="chat-markdown">{parseMarkdown(text).map((block, index) => block.type === "code"
    ? <pre key={index}><code data-language={block.language}>{block.text}</code></pre>
    : block.type === "list" ? <ul key={index}>{block.text.split("\n").map((line, i) => <li key={i}>{inline(line.replace(/^\s*(?:[-*]|\d+\.)\s/, ""))}</li>)}</ul>
    : <p key={index}>{inline(block.text)}</p>)}</div>;
}
