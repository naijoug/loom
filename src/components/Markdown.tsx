import { useMemo, type MouseEvent, type ReactNode } from "react";
import ReactMarkdown, { defaultUrlTransform, type Components } from "react-markdown";
import remarkCjkFriendly from "remark-cjk-friendly";
import remarkGfm from "remark-gfm";
import { isExternalUrl, openExternalLink } from "./externalLink";
import "./Markdown.css";

/** Replaces a fenced code block (e.g. Chat's `openui-lang` inline cards); `undefined` keeps the plain block. Pass a stable function. */
export type MarkdownCodeRenderer = (language: string | undefined, text: string) => ReactNode | undefined;
/** Learn and Research: `loom-src:` opens the source viewer, `loom-wiki:` another Learn page. Absent elsewhere, so Chat drops both schemes. */
export interface MarkdownLinks {
  onSource: (target: string) => void;
  onWiki: (target: string) => void;
  sourceStatus?: (target: string) => string | undefined;
}
const keepLoomLinks = (url: string) => (/^loom-(src|wiki):/.test(url) ? url : defaultUrlTransform(url));

type MdNode = { type: string; value?: string; children?: MdNode[] };
const FLOW_PARENTS = new Set(["root", "blockquote", "listItem", "footnoteDefinition"]);

/** Raw HTML is never parsed or executed: it shows as the literal text the agent wrote. */
function literalHtml() {
  const walk = (parent: MdNode) => {
    parent.children = parent.children?.map((child) => {
      if (child.type !== "html") {
        walk(child);
        return child;
      }
      const text = { type: "text", value: child.value ?? "" };
      return FLOW_PARENTS.has(parent.type) ? { type: "paragraph", children: [text] } : text;
    });
  };
  return (tree: MdNode) => walk(tree);
}

type HastNode = {
  type: string;
  value?: string;
  tagName?: string;
  properties?: Record<string, unknown>;
  children?: HastNode[];
};
const textOf = (node?: HastNode): string =>
  node?.type === "text" ? (node.value ?? "") : (node?.children ?? []).map(textOf).join("");

function ExternalLink({ href, children }: { href?: string; children: ReactNode }) {
  if (!isExternalUrl(href)) return <span className="chat-link-text">{children}</span>;
  const open = (event: MouseEvent) => {
    event.preventDefault();
    openExternalLink(href);
  };
  return (
    <a href={href} title={href} target="_blank" rel="noopener noreferrer" onClick={open}>
      {children}
    </a>
  );
}

function LoomLink({ href, links, children }: { href: string; links: MarkdownLinks; children: ReactNode }) {
  const [scheme, ...rest] = href.split(":");
  const target = rest.join(":");
  if (scheme === "loom-wiki")
    return (
      <a
        href="#"
        className="loom-wiki-link"
        onClick={(event) => {
          event.preventDefault();
          links.onWiki(target);
        }}
      >
        {children}
      </a>
    );
  const status = links.sourceStatus?.(target) ?? "valid";
  return (
    <button
      type="button"
      className="loom-src-link"
      data-status={status}
      title={target}
      onClick={() => links.onSource(target)}
    >
      {children}
    </button>
  );
}

function components(renderCode?: MarkdownCodeRenderer, links?: MarkdownLinks): Components {
  return {
    pre({ node }) {
      const code = (node as HastNode | undefined)?.children?.find((child) => child.tagName === "code");
      const classes = code?.properties?.className;
      const language = (Array.isArray(classes) ? classes : [])
        .map(String)
        .find((name) => name.startsWith("language-"))
        ?.slice(9);
      const text = textOf(code).replace(/\n$/, "");
      const custom = renderCode?.(language, text);
      if (custom !== undefined) return custom;
      return (
        <pre>
          <code data-language={language}>{text}</code>
        </pre>
      );
    },
    code: ({ children }) => <code>{children}</code>,
    a: ({ href, children }) =>
      links && href && /^loom-(src|wiki):/.test(href) ? (
        <LoomLink href={href} links={links}>
          {children}
        </LoomLink>
      ) : (
        <ExternalLink href={href}>{children}</ExternalLink>
      ),
    // Remote images would leave the app (and CSP blocks them); show them as links instead.
    img: ({ src, alt }) => <ExternalLink href={typeof src === "string" ? src : undefined}>{alt || src}</ExternalLink>,
    input: ({ type, checked }) =>
      type === "checkbox" ? <input type="checkbox" checked={!!checked} disabled readOnly /> : null,
  };
}

const REMARK_PLUGINS = [remarkGfm, remarkCjkFriendly, literalHtml];

export function Markdown({
  text,
  renderCode,
  links,
}: {
  text: string;
  renderCode?: MarkdownCodeRenderer;
  links?: MarkdownLinks;
}) {
  const overrides = useMemo(() => components(renderCode, links), [renderCode, links]);
  return (
    <div className="chat-markdown">
      <ReactMarkdown
        remarkPlugins={REMARK_PLUGINS}
        components={overrides}
        urlTransform={links ? keepLoomLinks : defaultUrlTransform}
      >
        {text}
      </ReactMarkdown>
    </div>
  );
}
