import { CheckCircle2, ShieldCheck } from "lucide-react";
import type { CommandRun } from "../../domain";
import { Button } from "../../components/common/Button";
import { WORKFLOW_COPY } from "../../copy/workflow";
import { shortTime, statusLabel } from "./model";

interface ValidationGateProps {
  gate: { tone: string; title: string; copy: string };
  validationCommandLabel: string;
  successfulRun?: CommandRun;
  failedRun?: CommandRun;
  blockingFailure?: CommandRun;
  canAccept: boolean;
  readOnly: boolean;
  onAccept: () => void;
}

export function ValidationGate({
  gate,
  validationCommandLabel,
  successfulRun,
  failedRun,
  blockingFailure,
  canAccept,
  readOnly,
  onAccept,
}: ValidationGateProps) {
  return (
    <section className={`debug-card testing-gate-card gate-${gate.tone}`}>
      <div className="debug-card-label"><ShieldCheck size={14} />验收门禁</div>
      <strong>{gate.title}</strong>
      <p>{gate.copy}</p>
      <div className="testing-evidence-grid">
        <div><span>验证命令</span><strong>{validationCommandLabel}</strong></div>
        <div><span>最近验证</span><strong>{statusLabel(successfulRun ?? failedRun)}</strong></div>
        <div><span>通过证据</span><strong>{successfulRun ? shortTime(successfulRun.endedAtMs) : "缺失"}</strong></div>
        <div><span>更新失败</span><strong>{blockingFailure ? shortTime(blockingFailure.startedAtMs) : "无"}</strong></div>
      </div>
      {!readOnly && (
        <>
          <Button
            type="button"
            variant="primary"
            iconLeft={<CheckCircle2 size={14} />}
            disabled={!canAccept}
            onClick={onAccept}
          >
            {WORKFLOW_COPY.actions.acceptTask}
          </Button>
          {!canAccept && (
            <div className="testing-accept-note">
              <strong>{WORKFLOW_COPY.blockers.acceptancePrefix}</strong>
              {gate.copy}
            </div>
          )}
        </>
      )}
    </section>
  );
}
