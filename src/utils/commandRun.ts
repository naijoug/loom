import type { CommandRun } from "../domain";

export function commandRunIntent(run: CommandRun) {
  return run.intent ?? "legacy";
}

export function isValidationRun(run: CommandRun, validationCommands: Set<string>) {
  if (run.intent === "validation") {
    return true;
  }

  if (!run.intent || run.intent === "legacy") {
    return validationCommands.has(run.command);
  }

  return false;
}

export function isFailedValidationRun(run: CommandRun, validationCommands: Set<string>) {
  return isValidationRun(run, validationCommands) && (run.status === "failed" || Boolean(run.errorSummary?.failed));
}

export function isSuccessfulValidationRun(run: CommandRun, validationCommands: Set<string>) {
  return isValidationRun(run, validationCommands) && run.status === "succeeded";
}
