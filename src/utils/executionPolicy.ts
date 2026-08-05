import { invokeCommand, TAURI_COMMANDS } from "../api";
import type {
  CommandSpec,
  ExecutionApproval,
  ExecutionAssessment,
  ExecutionRequest,
  PtySpec,
} from "../domain";

function requestFromSpec(spec: CommandSpec | PtySpec): ExecutionRequest {
  return {
    program: spec.program,
    args: spec.args,
    cwd: spec.cwd,
    projectPath: spec.projectPath ?? spec.cwd,
    agentId: "agentId" in spec ? spec.agentId : undefined,
  };
}

export async function authorizeExecution(spec: CommandSpec | PtySpec) {
  const request = requestFromSpec(spec);
  const assessment = await invokeCommand<ExecutionAssessment>(TAURI_COMMANDS.assessExecution, {
    request,
  });
  if (assessment.decision === "denied") {
    throw new Error(assessment.detail);
  }
  if (assessment.decision === "allowed") {
    return undefined;
  }

  const confirmed = window.confirm(
    [
      "Loom requires approval for this command.",
      "",
      `${spec.program} ${spec.args.join(" ")}`.trim(),
      `Working directory: ${assessment.normalizedCwd ?? spec.cwd}`,
      "",
      assessment.detail,
    ].join("\n"),
  );
  if (!confirmed) {
    throw new Error("Command was not run because approval was declined.");
  }

  const approval = await invokeCommand<ExecutionApproval>(TAURI_COMMANDS.approveExecution, {
    request,
  });
  return approval.id;
}
