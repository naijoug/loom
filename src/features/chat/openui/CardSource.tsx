import { useT } from "../../../i18n";

/** Raw openui-lang source, shown while loading, on request, or when a card cannot render. */
export function CardSource({ source }: { source: string }) {
  return <pre><code data-language="openui-lang">{source}</code></pre>;
}

export function CardFallback({ source }: { source: string }) {
  const t = useT();
  return <div className="chat-inline-card" data-fallback="true">
    <p className="chat-inline-card-note" role="note">{t("cardFallback")}</p>
    <CardSource source={source} />
  </div>;
}
