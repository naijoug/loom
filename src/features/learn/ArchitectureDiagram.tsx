import { useId, useLayoutEffect, useRef, useState } from "react";
import type { LearnArchitecture, LearnPageInfo } from "../../api/learn";
import { useT } from "../../i18n";
import { PageIcon, pageState } from "./LearningPath";

type Edge = { from: string; to: string; kind: "dep" | "user" };

/**
 * Layers as rows (title above), modules as wrapping HTML boxes. No edges by default: hovering or focusing a
 * module draws its dependencies and dependents over the rows and dims the rest. Clicking a module opens its page.
 */
export function ArchitectureDiagram({
  architecture,
  pages,
  read,
  running,
  onOpen,
}: {
  architecture: LearnArchitecture;
  pages: LearnPageInfo[];
  read: Set<string>;
  running?: string;
  onOpen: (pageId: string) => void;
}) {
  const t = useT();
  const marker = useId();
  const figure = useRef<HTMLElement>(null);
  const [focus, setFocus] = useState<string>();
  const [paths, setPaths] = useState<(Edge & { d: string })[]>([]);
  const byId = new Map(architecture.modules.map((module) => [module.id, module]));
  const pageById = new Map(pages.map((page) => [page.pageId, page]));
  const deps = focus ? (byId.get(focus)?.dependsOn ?? []).filter((id) => byId.has(id) && id !== focus) : [];
  const users = focus
    ? architecture.modules.filter((module) => module.id !== focus && module.dependsOn.includes(focus)).map((m) => m.id)
    : [];
  const role = (id: string) =>
    !focus ? undefined : id === focus ? "self" : deps.includes(id) ? "dep" : users.includes(id) ? "user" : "dim";
  const edgeKey = `${focus}|${deps.join()}|${users.join()}`;

  useLayoutEffect(() => {
    const root = figure.current;
    if (!focus || !root) {
      setPaths([]);
      return;
    }
    const edges: Edge[] = [
      ...deps.map((to) => ({ from: focus, to, kind: "dep" as const })),
      ...users.map((from) => ({ from, to: focus, kind: "user" as const })),
    ];
    const draw = () => {
      const box = root.getBoundingClientRect();
      const nodes = [...root.querySelectorAll<HTMLElement>("[data-module]")];
      const rect = (id: string) => nodes.find((node) => node.dataset.module === id)?.getBoundingClientRect();
      setPaths(
        edges.flatMap(({ from, to, kind }, index) => {
          const a = rect(from),
            b = rect(to);
          if (!a || !b) return [];
          // Spread the ends on the focused module across its width so the edges do not meet in one point.
          const spread = (r: DOMRect, id: string) =>
            r.left - box.left + r.width * (id === focus ? (index + 1) / (edges.length + 1) : 0.5);
          const x1 = spread(a, from),
            x2 = spread(b, to);
          if (Math.abs(b.top - a.top) < 4) {
            // Same row: dip into the gap below the row instead of crossing the boxes in between.
            const y = a.bottom - box.top,
              dip = y + 24;
            return [{ from, to, kind, d: `M${x1},${y} C${x1},${dip} ${x2},${dip} ${x2},${b.bottom - box.top + 1}` }];
          }
          const down = b.top > a.top;
          const y1 = (down ? a.bottom : a.top) - box.top,
            y2 = (down ? b.top : b.bottom) - box.top,
            bend = (y2 - y1) / 2;
          return [{ from, to, kind, d: `M${x1},${y1} C${x1},${y1 + bend} ${x2},${y2 - bend} ${x2},${y2}` }];
        }),
      );
    };
    draw();
    const observer = new ResizeObserver(draw);
    observer.observe(root);
    return () => observer.disconnect();
    // edgeKey captures focus, deps and users.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [edgeKey]);

  return (
    <figure className="learn-diagram" ref={figure} data-focus={focus ? true : undefined}>
      <div className="learn-diagram-board" role="group" aria-label={t("learnDiagramLabel")}>
        <svg className="learn-diagram-edges" aria-hidden="true">
          <defs>
            {(["dep", "user"] as const).map((kind) => (
              <marker
                key={kind}
                id={`${marker}-${kind}`}
                viewBox="0 0 8 8"
                refX="7"
                refY="4"
                markerWidth="7"
                markerHeight="7"
                orient="auto-start-reverse"
              >
                <path d="M0,0 L8,4 L0,8 z" className={`learn-diagram-arrow-${kind}`} />
              </marker>
            ))}
          </defs>
          {paths.map((path) => (
            <path
              key={`${path.from}>${path.to}`}
              d={path.d}
              className={`learn-diagram-edge learn-diagram-edge-${path.kind}`}
              markerEnd={`url(#${marker}-${path.kind})`}
            />
          ))}
        </svg>
        {architecture.layers.map((layer) => (
          <section key={layer.id} className="learn-diagram-layer">
            <h4>{layer.title}</h4>
            <div className="learn-diagram-row">
              {layer.moduleIds.map((id) => {
                const module = byId.get(id);
                if (!module) return null;
                const pageId = `module:${id}`;
                const page = pageById.get(pageId);
                const state = page ? pageState(page, read, running) : "pending";
                return (
                  <button
                    key={id}
                    type="button"
                    className="learn-diagram-node"
                    data-module={id}
                    data-role={role(id)}
                    data-state={state}
                    onMouseEnter={() => setFocus(id)}
                    onMouseLeave={() => setFocus((current) => (current === id ? undefined : current))}
                    onFocus={() => setFocus(id)}
                    onBlur={() => setFocus((current) => (current === id ? undefined : current))}
                    onClick={() => onOpen(pageId)}
                  >
                    <strong>{module.title}</strong>
                    <small>
                      <span className="learn-diagram-level">L{module.level}</span>
                      <PageIcon state={state} />
                    </small>
                  </button>
                );
              })}
            </div>
          </section>
        ))}
      </div>
      <figcaption>
        <span>
          <i className="learn-swatch" aria-hidden="true" />
          {t("learnDependsOn")}
        </span>
        <span>
          <i className="learn-swatch learn-swatch-user" aria-hidden="true" />
          {t("learnUsedBy")}
        </span>
        <span>{t("learnDiagramCaption")}</span>
      </figcaption>
    </figure>
  );
}

/**
 * The stored Architecture page repeats the diagram as a layer → module list. Keep its title for the page heading
 * and its module notes (summary, paths, dependencies, key source), dropping the layer and section headings.
 */
export function architectureNotes(markdown: string) {
  const lines = markdown.split("\n");
  const at = lines.findIndex((line) => line.startsWith("# "));
  const title = at < 0 ? "" : lines[at].slice(2).trim();
  const body = lines
    .filter((_, index) => index !== at)
    .filter((line) => !/^#{2,3} /.test(line))
    .map((line) => line.replace(/^#### /, "### "))
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
  return { title, body };
}
