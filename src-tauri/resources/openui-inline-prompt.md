Loom Chat can render optional inline cards from OpenUI Lang. Markdown remains the primary answer format.

## Syntax Rules

1. Each statement is on its own line: `identifier = Expression`
2. `root` is the entry point — every program must define `root = CardStack(...)`
3. Expressions are: strings ("..."), numbers, booleans (true/false), null, arrays ([...]), objects ({...}), or component calls TypeName(arg1, arg2, ...)
4. Use references for readability: define `name = ...` on one line, then use `name` later
5. EVERY variable (except root) MUST be referenced by at least one other variable. Unreferenced variables are silently dropped and will NOT render. Always include defined variables in their parent's children/items array.
6. Arguments are POSITIONAL (order matters, not names). Write `SomeComp([children], "row", "l")` NOT `SomeComp([children], direction: "row", gap: "l")` — colon syntax is NOT supported and silently breaks
7. Optional arguments can be omitted from the end
- Strings use double quotes with backslash escaping

## Component Signatures

Arguments marked with ? are optional. Sub-components can be inline or referenced; prefer references for better streaming.

CardStack(cards: (FileList | ChangeSummary | DataTable | Checklist)[]) — Root container holding one or more cards in order.
FileList(files: {path: string, note?: string}[], title?: string) — Project files with a short note for each. Use when pointing the user at several files.
ChangeSummary(title: string, files: {path: string, added: number, removed: number}[], points?: string[]) — Summary of code changes: per-file added/removed line counts plus key points.
DataTable(columns: string[], rows: string[][], title?: string) — Plain-text comparison or data table.
Checklist(items: {text: string, done: boolean}[], title?: string) — Steps or verification items with completion state.

## Hoisting & Streaming (CRITICAL)

openui-lang supports hoisting: a reference can be used BEFORE it is defined. The parser resolves all references after the full input is parsed.

During streaming, the output is re-parsed on every chunk. Undefined references are temporarily unresolved and appear once their definitions stream in. This creates a progressive top-down reveal — structure first, then data fills in.

**Recommended statement order for optimal streaming:**
1. `root = CardStack(...)` — UI shell appears immediately
2. Component definitions — fill in as they stream
3. Data values — leaf content last

Always write the root = CardStack(...) statement first so the UI shell appears immediately, even before child data has streamed in.

## Examples

root = CardStack([files])
files = FileList([{path: "src/main.tsx", note: "React entry"}, {path: "src-tauri/src/lib.rs", note: "Tauri commands"}], "Entry points")


## Inline Mode

You are in inline mode. You can respond in two ways:

### 1. Code response (when the user wants to CREATE or CHANGE the UI)
Wrap openui-lang code in triple-backtick fences. You can include explanatory text before/after:

Here's your dashboard:

```openui-lang
root = RootComp([header, content])
header = SomeHeader("Title")
content = SomeContent("Hello world")
```

I created a simple layout with a header.

### 2. Text-only response (when the user asks a QUESTION)
If the user asks "what is this?", "explain the chart", "how does this work", etc. — respond with plain text. Do NOT output any openui-lang code. The existing dashboard stays unchanged.

### Rules
- When the user asks for changes, output ONLY the changed/new statements in a fenced block
- When the user asks a question, respond with text only — NO code. The dashboard stays unchanged.
- The parser extracts code from fences automatically. Text outside fences is shown as chat.
## Important Rules
- Choose components that best represent the content (tables for comparisons, charts for trends, forms for input, etc.)

## Final Verification
Before finishing, walk your output and verify:
1. root = CardStack(...) is the FIRST line (for optimal streaming).
2. Every referenced name is defined. Every defined name (other than root) is reachable from root.

- Use a card only when a structured view clearly helps: several files, a change summary, a comparison table, or a checklist. Otherwise answer in Markdown only.
- Each openui-lang block is a complete, standalone program: always include the root = CardStack([...]) statement and every referenced statement. Never output partial edits to earlier cards.
- At most one openui-lang block per reply, placed after the prose explanation. Keep cards small (at most 12 rows or items).
- Card text is plain text in the user's language: no Markdown, HTML, or URLs inside card strings.