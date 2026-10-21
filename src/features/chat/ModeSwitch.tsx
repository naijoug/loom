import type { ChatPermissionMode } from "../../domain";
import { useT } from "../../i18n";
import { ModeIcon } from "./icons";
import { toWorkMode, toPermissionMode } from "./workMode";
export function ModeSwitch({ value, onChange, disabled = false, cards = false }: {
  value: ChatPermissionMode; onChange: (mode: ChatPermissionMode) => void; disabled?: boolean; cards?: boolean;
}) {
  const t = useT();
  return <div role="radiogroup" aria-label={t("mode")} className={cards ? "chat-mode-cards" : "chat-mode-switch"}>
    {(["research", "code"] as const).map(mode => <button type="button" key={mode} role="radio" data-mode={mode}
      aria-checked={toWorkMode(value) === mode} disabled={disabled}
      onClick={() => onChange(toPermissionMode(mode, value === "auto"))}>
      {cards ? <>
        <span className="chat-mode-card-icon"><ModeIcon mode={mode} size={16} /></span>
        <strong>{t(mode)}</strong>
        <small>{t(mode === "research" ? "researchHint" : "codeHint")}</small>
      </> : <><ModeIcon mode={mode} size={13} />{t(mode)}</>}
    </button>)}
  </div>;
}
