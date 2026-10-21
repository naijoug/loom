import { useMemo, useState } from "react";
import { Renderer } from "@openuidev/react-lang";
import { useT } from "../../../i18n";
import { CardFallback, CardSource } from "./CardSource";
import type { InlineCardProps } from "./InlineCard";
import { checkInlineCard, inlineCardLibrary, inlineCardText } from "./library";

/**
 * Streaming cards render progressively. A finished card renders only when the parser accepts
 * it; the Renderer would otherwise show an empty container for invalid input.
 */
export default function InlineCardView({ source, streaming, onInsert }: InlineCardProps) {
  const t = useT();
  const [showSource, setShowSource] = useState(false);
  const [copied, setCopied] = useState(false);
  const check = useMemo(() => streaming ? null : checkInlineCard(source), [source, streaming]);
  const text = useMemo(() => check?.ok ? inlineCardText(source) : "", [check, source]);
  if (check && !check.ok) return <CardFallback source={source} />;
  async function copy() {
    try { await navigator.clipboard.writeText(text); setCopied(true); setTimeout(() => setCopied(false), 1500); }
    catch (error) { console.warn("copy failed", error); }
  }
  return <div className="chat-inline-card" data-streaming={streaming || undefined}>
    {showSource ? <CardSource source={source} /> : <Renderer response={source} library={inlineCardLibrary} isStreaming={streaming} />}
    {check?.ok && <div className="chat-inline-card-actions">
      <button type="button" aria-pressed={showSource} onClick={() => setShowSource(value => !value)}>{t(showSource ? "cardShowCard" : "cardShowSource")}</button>
      <button type="button" onClick={() => void copy()}>{t(copied ? "cardCopied" : "cardCopy")}</button>
      {onInsert && <button type="button" onClick={() => onInsert(text)}>{t("cardInsert")}</button>}
    </div>}
  </div>;
}
