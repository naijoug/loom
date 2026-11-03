// Remove inline test modules without dropping the production code after them.
// The mask preserves offsets/newlines and hides braces in Rust comments and literals.
export function rustCodeMask(text) {
  let result = "",
    i = 0;
  const hide = (end) => {
    result += text.slice(i, end).replace(/[^\n]/g, " ");
    i = end;
  };
  while (i < text.length) {
    if (text.startsWith("//", i)) {
      const end = text.indexOf("\n", i);
      hide(end < 0 ? text.length : end);
      continue;
    }
    if (text.startsWith("/*", i)) {
      let depth = 1,
        end = i + 2;
      while (end < text.length && depth) {
        if (text.startsWith("/*", end)) {
          depth++;
          end += 2;
        } else if (text.startsWith("*/", end)) {
          depth--;
          end += 2;
        } else end++;
      }
      hide(end);
      continue;
    }
    const raw = /^(?:br|cr|r)(#*)"/.exec(text.slice(i));
    if (raw) {
      const close = `"${raw[1]}`,
        end = text.indexOf(close, i + raw[0].length);
      hide(end < 0 ? text.length : end + close.length);
      continue;
    }
    if (text[i] === '"') {
      let end = i + 1;
      while (end < text.length) {
        if (text[end] === "\\") end += 2;
        else if (text[end++] === '"') break;
      }
      hide(Math.min(end, text.length));
      continue;
    }
    const char = /^'(?:\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.)|[^'\\\n])'/u.exec(text.slice(i));
    if (char) {
      hide(i + char[0].length);
      continue;
    }
    result += text[i++];
  }
  return result;
}
export function productionCode(file, text) {
  if (/(^|_)tests?\.rs$/.test(file)) return "";
  const mask = rustCodeMask(text);
  const pattern = /#\[cfg\((?:all\()?test\b[^\]]*\]\s*(?:pub(?:\([a-z]+\))?\s+)?mod\s+\w+\s*(\{|;)/g;
  let result = "",
    previous = 0,
    match;
  while ((match = pattern.exec(mask))) {
    let end = pattern.lastIndex;
    if (match[1] === "{") {
      let depth = 1;
      while (end < mask.length && depth) {
        if (mask[end] === "{") depth++;
        else if (mask[end] === "}") depth--;
        end++;
      }
    }
    result += text.slice(previous, match.index) + text.slice(match.index, end).replace(/[^\n]/g, " ");
    previous = end;
    pattern.lastIndex = end;
  }
  return result + text.slice(previous);
}
