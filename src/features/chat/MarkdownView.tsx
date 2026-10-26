import { useMemo, type MouseEvent, type ReactNode } from "react";
import ReactMarkdown, { type Components } from "react-markdown";
import remarkCjkFriendly from "remark-cjk-friendly";
import remarkGfm from "remark-gfm";
import { isExternalUrl, openExternalLink } from "./externalLink";
import { InlineCard } from "./openui/InlineCard";

/** Present when experimental inline cards are on; `openui-lang` blocks then render as cards. */
export interface MarkdownCards { streaming: boolean; onInsert?: (text: string) => void }

type MdNode = { type: string; value?: string; children?: MdNode[] };
const FLOW_PARENTS = new Set(["root", "blockquote", "listItem", "footnoteDefinition"]);

/** Raw HTML is never parsed or executed: it shows as the literal text the agent wrote. */
function literalHtml() {
  const walk = (parent: MdNode) => {
    parent.children = parent.children?.map(child => {
      if (child.type !== "html") { walk(child); return child; }
      const text = { type: "text", value: child.value ?? "" };
      return FLOW_PARENTS.has(parent.type) ? { type: "paragraph", children: [text] } : text;
    });
  };
  return (tree: MdNode) => walk(tree);
}

type HastNode = { type: string; value?: string; tagName?: string; properties?: Record<string, unknown>; children?: HastNode[] };
const textOf = (node?: HastNode): string => node?.type === "text" ? node.value ?? "" : (node?.children ?? []).map(textOf).join("");

function ExternalLink({ href, children }: { href?: string; children: ReactNode }) {
  if (!isExternalUrl(href)) return <span className="chat-link-text">{children}</span>;
  const open = (event: MouseEvent) => { event.preventDefault(); openExternalLink(href); };
  return <a href={href} title={href} target="_blank" rel="noopener noreferrer" onClick={open}>{children}</a>;
}

function components(cards?: MarkdownCards): Components {
  return {
    pre({ node }) {
      const code = (node as HastNode | undefined)?.children?.find(child => child.tagName === "code");
      const classes = code?.properties?.className;
      const language = (Array.isArray(classes) ? classes : []).map(String).find(name => name.startsWith("language-"))?.slice(9);
      const text = textOf(code).replace(/\n$/, "");
      if (cards && language === "openui-lang") return <InlineCard source={text} streaming={cards.streaming} onInsert={cards.onInsert} />;
      return <pre><code data-language={language}>{text}</code></pre>;
    },
    code: ({ children }) => <code>{children}</code>,
    a: ({ href, children }) => <ExternalLink href={href}>{children}</ExternalLink>,
    // Remote images would leave the app (and CSP blocks them); show them as links instead.
    img: ({ src, alt }) => <ExternalLink href={typeof src === "string" ? src : undefined}>{alt || src}</ExternalLink>,
    input: ({ type, checked }) => type === "checkbox" ? <input type="checkbox" checked={!!checked} disabled readOnly /> : null,
  };
}

const REMARK_PLUGINS = [remarkGfm, remarkCjkFriendly, literalHtml];

export function Markdown({ text, cards }: { text: string; cards?: MarkdownCards }) {
  const enabled = !!cards, streaming = !!cards?.streaming, onInsert = cards?.onInsert;
  const overrides = useMemo(() => components(enabled ? { streaming, onInsert } : undefined), [enabled, streaming, onInsert]);
  return <div className="chat-markdown"><ReactMarkdown remarkPlugins={REMARK_PLUGINS} components={overrides}>{text}</ReactMarkdown></div>;
}
