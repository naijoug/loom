export interface ParsedCommandLine {
  program: string;
  args: string[];
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
