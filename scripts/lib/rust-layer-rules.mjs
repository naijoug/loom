import { rustCodeMask } from './rust-production.mjs';

// Match direct paths and grouped/aliased use trees, including `use crate as root`.
export function moduleReferences(source, modules) {
  const code = rustCodeMask(source);
  const roots = ['crate', 'loom_lib'];
  for (const match of code.matchAll(/\buse\s+(?:crate|loom_lib)\s+as\s+(\w+)\s*;/g)) roots.push(match[1]);
  return modules.filter(name => roots.some(root => {
    if (new RegExp(`\\b${root}\\s*::\\s*${name}\\b`).test(code)) return true;
    return [...code.matchAll(new RegExp(`\\buse\\s+${root}\\s*::\\s*\\{([^;]+)\\}\\s*;`, 'g'))]
      .some(match => new RegExp(`(?:^|[,{}])\\s*${name}\\b`).test(match[1]));
  }));
}

export function spawnsProcess(source) {
  const code = rustCodeMask(source);
  // Detect fully qualified uses, module aliases and renamed imported Command.
  if (/\b(?:tokio|std)\s*::\s*process\s*::\s*(?:Command|\{[^;]*\bCommand\b)/.test(code)) return true;
  if (/\buse\s+(?:tokio|std)\s*::\s*process\s*(?:as\s+\w+)?\s*;/.test(code)) return true;
  if (/\buse\s+(?:std|tokio)\s*::\s*\{[^;]*\bprocess\b/.test(code)) return true;
  return /\bCommand\s*::\s*new\s*\(/.test(code);
}
