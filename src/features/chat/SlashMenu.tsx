import { useT } from "../../i18n";
export function SlashMenu({ commands, selected, onSelect }: { commands: readonly string[]; selected: number; onSelect: (command: string) => void }) {
  const t = useT();
  return <div className="chat-slash-menu" role="listbox" id="chat-slash-menu" aria-label={t("commands")}>
    {commands.map((command, index) => <button id={`chat-slash-${index}`} role="option" aria-selected={selected === index}
      type="button" key={command} onMouseDown={event => event.preventDefault()} onClick={() => onSelect(command)}>{command}</button>)}
  </div>;
}
