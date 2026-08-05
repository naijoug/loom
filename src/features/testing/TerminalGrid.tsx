import { Plus, RotateCcw } from "lucide-react";
import type { CommandLogEvent, CommandRun, TerminalSlot } from "../../domain";
import { draftFromTerminalSlot } from "../../utils/terminalSlots";
import { TerminalCard } from "../../components/TaskDetail/TerminalCard";
import { slotEmptyMessage, slotEndpoint } from "./model";

interface TerminalGridProps {
  slots: TerminalSlot[];
  readOnly: boolean;
  commandError: string | null;
  autoNotice: string | null;
  slotRun: (slot: TerminalSlot) => CommandRun | undefined;
  logsForRun: (runId: string) => CommandLogEvent[];
  onRun: (slot: TerminalSlot) => void;
  onStop: (slot: TerminalSlot, run?: CommandRun) => void;
  onEdit: (slot: TerminalSlot) => void;
  onRemove: (slot: TerminalSlot) => void;
  onQuote: (text: string, command: string) => void;
  onAdd: () => void;
  onReset: () => void;
}

export function TerminalGrid({
  slots,
  readOnly,
  commandError,
  autoNotice,
  slotRun,
  logsForRun,
  onRun,
  onStop,
  onEdit,
  onRemove,
  onQuote,
  onAdd,
  onReset,
}: TerminalGridProps) {
  return (
    <div className="testing-terminals">
      {slots.map((slot) => {
        const run = slotRun(slot);
        return (
          <TerminalCard
            key={slot.id}
            title={slot.name}
            command={slot.command}
            endpoint={slotEndpoint(slot, run)}
            tone={slot.kind === "preview" ? "frontend" : "validation"}
            mode={slot.kind === "preview" ? "pty" : "logs"}
            emptyMessage={slotEmptyMessage(slot)}
            run={run}
            logs={slot.kind === "validation" && run ? logsForRun(run.id) : []}
            onRun={() => onRun(slot)}
            onStop={() => onStop(slot, run)}
            onEdit={readOnly ? undefined : () => onEdit(slot)}
            onRemove={readOnly ? undefined : () => onRemove(slot)}
            onQuote={readOnly ? undefined : onQuote}
            disabled={readOnly}
          />
        );
      })}

      {!readOnly && (
        <div className="testing-terminal-actions">
          <button type="button" className="testing-add-terminal" onClick={onAdd}>
            <Plus size={14} />
            Add terminal
          </button>
          <button
            type="button"
            className="testing-add-terminal"
            title="Re-scan the project and replace terminals with detected commands"
            onClick={onReset}
          >
            <RotateCcw size={13} />
            Reset to detected
          </button>
        </div>
      )}

      {commandError && <div className="testing-inline-error">{commandError}</div>}
      {autoNotice && <div className="testing-inline-note">{autoNotice}</div>}
    </div>
  );
}

export { draftFromTerminalSlot };
