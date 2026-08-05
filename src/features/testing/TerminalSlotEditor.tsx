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
        <h2>{draft.id ? "编辑终端" : "添加终端"}</h2>
        <label className="testing-slot-field">
          <span>名称</span>
          <input
            value={draft.name}
            autoFocus
            placeholder="预览"
            onChange={(event) => setDraft({ ...draft, name: event.target.value })}
          />
        </label>
        <label className="testing-slot-field">
          <span>命令</span>
          <input
            value={draft.command}
            placeholder="pnpm dev"
            onChange={(event) => setDraft({ ...draft, command: event.target.value })}
          />
        </label>
        <label className="testing-slot-field">
          <span>工作目录（可选，相对于项目）</span>
          <input
            value={draft.cwd}
            placeholder="项目根目录，例如 todo-react"
            onChange={(event) => setDraft({ ...draft, cwd: event.target.value })}
          />
        </label>
        <label className="testing-slot-field">
          <span>类型</span>
          <select
            value={draft.kind}
            onChange={(event) => setDraft({ ...draft, kind: event.target.value as TerminalSlot["kind"] })}
          >
            <option value="preview">预览（实时开发服务）</option>
            <option value="validation">验证（一次性检查）</option>
          </select>
        </label>
        <p className="testing-slot-hint">
          {draft.kind === "preview"
            ? "在支持颜色的真实终端（PTY）中长期运行，不参与验收门禁。"
            : "一次性管道命令，其退出码和日志会驱动验收门禁。"}
        </p>
        <div className="confirm-modal-actions">
          <Button variant="ghost" onClick={onClose}>取消</Button>
          <Button
            variant="primary"
            onClick={onSave}
            disabled={!draft.name.trim() || !draft.command.trim()}
          >
            保存
          </Button>
        </div>
      </div>
    </div>
  );
}
