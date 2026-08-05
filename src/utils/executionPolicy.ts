import { invoke } from "@tauri-apps/api/core";
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
  const assessment = await invoke<ExecutionAssessment>("assess_execution", { request });
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

  const approval = await invoke<ExecutionApproval>("approve_execution", { request });
  return approval.id;
}
