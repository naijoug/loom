export interface ParsedCommandLine {
  program: string;
  args: string[];
}

export type DangerousCommandReason =
  | "destructive-remove"
  | "git-reset-hard"
  | "dependency-install"
  | "production-target";

export interface DangerousCommandFinding {
  reason: DangerousCommandReason;
  detail: string;
}

export function parseCommandLine(input: string): ParsedCommandLine {
  const tokens: string[] = [];
  let current = "";
  let quote: '"' | "'" | null = null;
  let escaping = false;
  let tokenStarted = false;

  for (const char of input.trim()) {
    if (escaping) {
      current += char;
      escaping = false;
      tokenStarted = true;
      continue;
    }

    if (char === "\\" && quote !== "'") {
      escaping = true;
      tokenStarted = true;
      continue;
    }

    if ((char === '"' || char === "'") && quote === null) {
      quote = char;
      tokenStarted = true;
      continue;
    }

    if (char === quote) {
      quote = null;
      continue;
    }

    if (/\s/.test(char) && quote === null) {
      if (tokenStarted) {
        tokens.push(current);
        current = "";
        tokenStarted = false;
      }
      continue;
    }

    current += char;
    tokenStarted = true;
  }

  if (escaping) {
    current += "\\";
  }

  if (quote) {
    throw new Error(`Unclosed ${quote === '"' ? "double" : "single"} quote in command`);
  }

  if (tokenStarted) {
    tokens.push(current);
  }

  const [program = "", ...args] = tokens;
  return { program, args };
}

export function detectDangerousCommand(input: string): DangerousCommandFinding | null {
  let parsed: ParsedCommandLine;
  try {
    parsed = parseCommandLine(input);
  } catch {
    return null;
  }

  const program = parsed.program.toLowerCase();
  const args = parsed.args.map((arg) => arg.toLowerCase());
  const normalized = [program, ...args].join(" ");

  if (
    program === "rm" &&
    args.some((arg) => /^-[a-z]*r[a-z]*f[a-z]*$/.test(arg) || /^-[a-z]*f[a-z]*r[a-z]*$/.test(arg))
  ) {
    return {
      reason: "destructive-remove",
      detail: "Deletes files recursively with force.",
    };
  }

  if (program === "git" && args[0] === "reset" && args.includes("--hard")) {
    return {
      reason: "git-reset-hard",
      detail: "Discards local git changes.",
    };
  }

  const packageManagerMutations = new Set(["add", "ci", "install", "remove", "uninstall", "update", "upgrade"]);
  const mutatesDependencies = (candidateArgs: string[]) => {
    const firstCommandIndex = candidateArgs.findIndex((arg) => !arg.startsWith("-"));
    return firstCommandIndex >= 0 && packageManagerMutations.has(candidateArgs[firstCommandIndex]);
  };
  const runsPipModule = args.includes("-m") && args.includes("pip");
  const pipModuleArgs = runsPipModule ? args.slice(args.indexOf("pip") + 1) : [];

  if (
    (["pnpm", "npm", "yarn", "bun"].includes(program) && mutatesDependencies(args)) ||
    (program === "cargo" && mutatesDependencies(args)) ||
    (program === "pip" && mutatesDependencies(args)) ||
    (["python", "python3"].includes(program) && runsPipModule && mutatesDependencies(pipModuleArgs))
  ) {
    return {
      reason: "dependency-install",
      detail: "Installs or changes project dependencies.",
    };
  }

  if (
    /\b(prod|production)\b/.test(normalized) ||
    /\bhttps?:\/\/[^/\s]*(prod|production)[^/\s]*/.test(normalized)
  ) {
    return {
      reason: "production-target",
      detail: "Mentions a production-like target.",
    };
  }

  return null;
}
