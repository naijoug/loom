import { useMemo } from "react";
import { Markdown, type MarkdownCodeRenderer } from "../../components/Markdown";
import { InlineCard } from "./openui/InlineCard";

/** Present when experimental inline cards are on; `openui-lang` blocks then render as cards. */
export interface MarkdownCards {
  streaming: boolean;
  onInsert?: (text: string) => void;
}

export function ChatMarkdown({ text, cards }: { text: string; cards?: MarkdownCards }) {
  const enabled = !!cards,
    streaming = !!cards?.streaming,
    onInsert = cards?.onInsert;
  const renderCode = useMemo<MarkdownCodeRenderer | undefined>(
    () =>
      enabled
        ? (language, source) =>
            language === "openui-lang" ? (
              <InlineCard source={source} streaming={streaming} onInsert={onInsert} />
            ) : undefined
        : undefined,
    [enabled, streaming, onInsert],
  );
  return <Markdown text={text} renderCode={renderCode} />;
}
