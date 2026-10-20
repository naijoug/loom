export const SLASH_COMMANDS = ["/new", "/agent", "/mode", "/clear-resume", "/export"] as const;
export type SlashCommand =
  | { command: "new" | "clear-resume" | "export" }
  | { command: "agent"; argument: string }
  | { command: "mode"; argument: "research" | "code" | "auto" };
export function parseSlashCommand(value: string): SlashCommand | null {
  const [command, argument, ...extra] = value.trim().split(/\s+/);
  if (extra.length) return null;
  if (["/new", "/clear-resume", "/export"].includes(command) && !argument) return { command: command.slice(1) as "new" | "clear-resume" | "export" };
  if (command === "/agent" && argument) return { command: "agent", argument };
  if (command === "/mode" && (argument === "research" || argument === "code" || argument === "auto")) return { command: "mode", argument };
  return null;
}
export function filterSlashCommands(value: string) {
  return value.startsWith("/") && !/\s/.test(value) ? SLASH_COMMANDS.filter(command => command.startsWith(value)) : [];
}
