import type { Dispatch, SetStateAction } from "react";
import type { TerminalSlot } from "../../domain";
import type { TerminalSlotDraft } from "../../utils/terminalSlots";
import { Button } from "../../components/common/Button";

interface TerminalSlotEditorProps {
  draft: TerminalSlotDraft;
  setDraft: Dispatch<SetStateAction<TerminalSlotDraft | null>>;
  onSave: () => void;
  onClose: () => void;
}

export function TerminalSlotEditor({ draft, setDraft, onSave, onClose }: TerminalSlotEditorProps) {
  return (
    <div className="confirm-backdrop" role="presentation" onClick={onClose}>
      <div
        className="confirm-modal testing-slot-modal"
        role="dialog"
        aria-modal="true"
        onClick={(event) => event.stopPropagation()}
      >
        <h2>{draft.id ? "Edit terminal" : "Add terminal"}</h2>
        <label className="testing-slot-field">
          <span>Name</span>
          <input
            value={draft.name}
            autoFocus
            placeholder="Preview"
            onChange={(event) => setDraft({ ...draft, name: event.target.value })}
          />
        </label>
        <label className="testing-slot-field">
          <span>Command</span>
          <input
            value={draft.command}
            placeholder="pnpm dev"
            onChange={(event) => setDraft({ ...draft, command: event.target.value })}
          />
        </label>
        <label className="testing-slot-field">
          <span>Working directory (optional, relative to project)</span>
          <input
            value={draft.cwd}
            placeholder="(project root) e.g. todo-react"
            onChange={(event) => setDraft({ ...draft, cwd: event.target.value })}
          />
        </label>
        <label className="testing-slot-field">
          <span>Kind</span>
          <select
            value={draft.kind}
            onChange={(event) => setDraft({ ...draft, kind: event.target.value as TerminalSlot["kind"] })}
          >
            <option value="preview">Preview (live dev server)</option>
            <option value="validation">Validation (one-shot check)</option>
          </select>
        </label>
        <p className="testing-slot-hint">
          {draft.kind === "preview"
            ? "Runs in a real terminal (PTY) with colors. Long-running; not used for the acceptance gate."
            : "Piped one-shot command. Its exit code and logs drive the acceptance gate."}
        </p>
        <div className="confirm-modal-actions">
          <Button variant="ghost" onClick={onClose}>Cancel</Button>
          <Button
            variant="primary"
            onClick={onSave}
            disabled={!draft.name.trim() || !draft.command.trim()}
          >
            Save
          </Button>
        </div>
      </div>
    </div>
  );
}
