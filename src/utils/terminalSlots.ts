import type { ProjectSummary, TerminalSlot, TerminalSlotKind } from "../domain";

export interface TerminalSlotDraft {
  id: string | null;
  name: string;
  command: string;
  kind: TerminalSlotKind;
  cwd: string;
}

export function newTerminalSlotId() {
  const randomId =
    typeof crypto !== "undefined" && typeof crypto.randomUUID === "function"
      ? crypto.randomUUID()
      : `${Date.now()}-${Math.random().toString(16).slice(2)}`;
  return `slot-${randomId}`;
}

function preferredPreviewCommand(project: ProjectSummary) {
  return (
    project.suggestedCommands.find((command) =>
      /\b(pnpm|npm|yarn)\s+(dev|run\s+dev)\b/.test(command),
    ) ?? "pnpm dev"
  );
}

function commandPriority(command: string) {
  if (/\b(test|smoke)\b/.test(command)) {
    return 0;
  }
  if (/\b(typecheck|check)\b/.test(command)) {
    return 1;
  }
  if (/\bbuild\b/.test(command)) {
    return 2;
  }
  if (/\blint\b/.test(command)) {
    return 3;
  }
  return 4;
}

function isValidationCommand(command: string) {
  return /\b(test|smoke|typecheck|check|build|lint)\b/.test(command);
}

function preferredValidationCommand(project: ProjectSummary) {
  const validationCommands = project.suggestedCommands
    .filter(isValidationCommand)
    .sort((left, right) => commandPriority(left) - commandPriority(right));

  if (validationCommands[0]) {
    return validationCommands[0];
  }
  if (project.detectedStacks.includes("Tauri")) {
    return "cargo check --manifest-path src-tauri/Cargo.toml";
  }
  if (project.detectedStacks.includes("Rust")) {
    return "cargo test";
  }
  if (project.detectedStacks.includes("Go")) {
    return "go test ./...";
  }
  return "pnpm test";
}

export function buildDefaultSlots(project: ProjectSummary): TerminalSlot[] {
  return [
    { id: newTerminalSlotId(), name: "Preview", command: preferredPreviewCommand(project), kind: "preview" },
    {
      id: newTerminalSlotId(),
      name: "Validation",
      command: preferredValidationCommand(project),
      kind: "validation",
    },
  ];
}

export function draftFromTerminalSlot(slot: TerminalSlot): TerminalSlotDraft {
  return {
    id: slot.id,
    name: slot.name,
    command: slot.command,
    kind: slot.kind,
    cwd: slot.cwd ?? "",
  };
}

export function blankTerminalSlotDraft(kind: TerminalSlotKind = "validation"): TerminalSlotDraft {
  return {
    id: null,
    name: "",
    command: "",
    kind,
    cwd: "",
  };
}

export function terminalSlotFromDraft(draft: TerminalSlotDraft): TerminalSlot | null {
  const name = draft.name.trim();
  const command = draft.command.trim();
  if (!name || !command) {
    return null;
  }

  const cwd = draft.cwd.trim();
  return {
    id: draft.id ?? newTerminalSlotId(),
    name,
    command,
    kind: draft.kind,
    cwd: cwd || undefined,
  };
}

export function applyTerminalSlotDraft(
  slots: TerminalSlot[],
  draft: TerminalSlotDraft,
): TerminalSlot[] | null {
  const slot = terminalSlotFromDraft(draft);
  if (!slot) {
    return null;
  }

  if (draft.id) {
    return slots.map((candidate) => (candidate.id === draft.id ? slot : candidate));
  }

  return [...slots, slot];
}

export function resolveTerminalSlotCwd(projectPath: string, slot: { cwd?: string }) {
  return slot.cwd ? `${projectPath}/${slot.cwd}` : projectPath;
}
