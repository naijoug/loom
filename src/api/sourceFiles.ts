/** Shapes of `source_files.rs`: bounded reads of a study checkout or a registered project, and `loom-src:` references. */
export interface RefStats {
  valid: number;
  invalid: number;
}
export interface SourceRef {
  target: string;
  path: string;
  startLine: number;
  endLine: number;
  status: "valid" | "missing" | "range" | "outside" | string;
}
export interface SourceText {
  path: string;
  text: string;
  firstLine: number;
  totalLines: number;
  truncated: boolean;
  language: string;
}
export interface TreeEntry {
  name: string;
  path: string;
  kind: "dir" | "file" | "symlink" | string;
  size: number;
}
export interface SourceTarget {
  path: string;
  startLine?: number;
  endLine?: number;
}

/** What the shared source viewer reads from; each feature binds it to its own commands. */
export interface SourceReader {
  readSource(path: string, startLine?: number, endLine?: number): Promise<SourceText>;
  listTree(dir?: string): Promise<TreeEntry[]>;
}

/** `loom-src:path#L3-L9` → path and range; mirrors `source_files::parse_target`. */
export function parseSourceTarget(target: string): SourceTarget {
  const [rawPath, anchor = ""] = target.split("#", 2);
  const path = decodeURIComponent(rawPath).replace(/^\.\//, "");
  const match = /^L(\d+)(?:-L?(\d+))?$/.exec(anchor);
  if (!match) return { path };
  const startLine = Number(match[1]);
  return { path, startLine, endLine: match[2] ? Math.max(startLine, Number(match[2])) : startLine };
}
