import type { ChatPermissionMode } from "../../domain";
import { useT } from "../../i18n";
import { toWorkMode, toPermissionMode } from "./workMode";
export function ModeSwitch({ value, onChange, disabled = false, cards = false }: {
  value: ChatPermissionMode; onChange: (mode: ChatPermissionMode) => void; disabled?: boolean; cards?: boolean;
}) {
  const t = useT();
  return <div role="radiogroup" aria-label={t("mode")} className={cards ? "chat-mode-cards" : "chat-mode-switch"}>
    {(["research", "code"] as const).map(mode => <button type="button" key={mode} role="radio"
      aria-checked={toWorkMode(value) === mode} disabled={disabled}
      onClick={() => onChange(toPermissionMode(mode, value === "auto"))}>
      <span aria-hidden="true">{mode === "research" ? "⌕" : "‹›"}</span> {t(mode)}
      {cards && <small>{t(mode === "research" ? "researchHint" : "codeHint")}</small>}
    </button>)}
  </div>;
}
