import { useT } from "../../i18n";
import type { MessageKey } from "../../i18n/en";
const descriptions: Record<string, MessageKey> = {
  "/new": "slashNew", "/agent": "slashAgent", "/mode": "slashMode", "/clear-resume": "slashClearResume", "/export": "slashExport",
};
export function SlashMenu({ commands, selected, onSelect }: { commands: readonly string[]; selected: number; onSelect: (command: string) => void }) {
  const t = useT();
  return <div className="chat-slash-menu">
    <div role="listbox" id="chat-slash-menu" aria-label={t("commands")}>
      {commands.map((command, index) => <button id={`chat-slash-${index}`} role="option" aria-selected={selected === index}
        type="button" key={command} onMouseDown={event => event.preventDefault()} onClick={() => onSelect(command)}>
        <code>{command}</code><span>{descriptions[command] ? t(descriptions[command]) : ""}</span>
      </button>)}
    </div>
    <p className="chat-kbd-hint" aria-hidden="true"><kbd>↑↓</kbd> {t("kbdSelect")} <kbd>Enter</kbd> {t("kbdRun")} <kbd>Esc</kbd> {t("kbdClose")}</p>
  </div>;
}
