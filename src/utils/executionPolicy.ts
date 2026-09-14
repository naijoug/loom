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

function describeApprovalCategory(category: ExecutionAssessment["category"]) {
  switch (category) {
    case "dependency_install":
      return "Dependency or environment change";
    case "destructive_filesystem":
      return "Destructive filesystem change";
    case "destructive_git":
      return "Destructive Git change";
    case "production_external":
      return "Production or external target";
    case "project_boundary":
      return "Project boundary violation";
    case "none":
      return "No recognized risk";
    default:
      return category;
  }
}

export function formatApprovalPrompt(spec: CommandSpec | PtySpec, assessment: ExecutionAssessment) {
  return [
    "Loom requires approval for this command.",
    "",
    `${spec.program} ${spec.args.join(" ")}`.trim(),
    `Working directory: ${assessment.normalizedCwd ?? spec.cwd}`,
    `Risk: ${describeApprovalCategory(assessment.category)} (${assessment.riskLevel})`,
    "",
    assessment.detail,
  ].join("\n");
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

  const confirmed = window.confirm(formatApprovalPrompt(spec, assessment));
  if (!confirmed) {
    throw new Error("Command was not run because approval was declined.");
  }

  const approval = await invokeCommand<ExecutionApproval>(TAURI_COMMANDS.approveExecution, {
    request,
  });
  return approval.id;
}
