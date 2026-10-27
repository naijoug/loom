import { useMemo } from "react";
import type { LearnArchitecture } from "../../api/learn";
import { useT } from "../../i18n";

const WIDTH = 760, ROW = 92, LABEL = 96, BOX_H = 44, GAP = 14, PAD = 12;

type Box = { id: string; title: string; level: number; x: number; y: number; w: number };

/** Layers as rows, modules as boxes, `dependsOn` as arrows. Clicking a box opens its page. */
export function ArchitectureDiagram({ architecture, readyPages, onOpen }: {
  architecture: LearnArchitecture; readyPages: Set<string>; onOpen: (pageId: string) => void;
}) {
  const t = useT();
  const { boxes, height } = useMemo(() => {
    const byId = new Map(architecture.modules.map(module => [module.id, module]));
    const result = new Map<string, Box>();
    architecture.layers.forEach((layer, row) => {
      const ids = layer.moduleIds.filter(id => byId.has(id));
      const available = WIDTH - LABEL - PAD * 2;
      const w = Math.min(168, (available - GAP * (ids.length - 1)) / Math.max(ids.length, 1));
      ids.forEach((id, index) => {
        const module = byId.get(id)!;
        result.set(id, { id, title: module.title, level: module.level, x: LABEL + PAD + index * (w + GAP), y: PAD + row * ROW + (ROW - BOX_H) / 2 - 6, w });
      });
    });
    return { boxes: result, height: PAD * 2 + architecture.layers.length * ROW };
  }, [architecture]);
  const edges = architecture.modules.flatMap(module => module.dependsOn
    .filter(target => boxes.has(module.id) && boxes.has(target))
    .map(target => ({ from: boxes.get(module.id)!, to: boxes.get(target)! })));
  return <figure className="learn-diagram">
    <svg viewBox={`0 0 ${WIDTH} ${height}`} role="img" aria-label={t("learnDiagramLabel")}>
      <defs><marker id="learn-arrow" viewBox="0 0 8 8" refX="7" refY="4" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0 L8,4 L0,8 z" className="learn-diagram-arrow" /></marker></defs>
      {architecture.layers.map((layer, row) => <g key={layer.id}>
        <rect x={PAD / 2} y={PAD + row * ROW} width={WIDTH - PAD} height={ROW - 10} rx={10} className="learn-diagram-layer" />
        <text x={PAD + 4} y={PAD + row * ROW + ROW / 2 - 2} className="learn-diagram-layer-title">{clip(layer.title, 13)}</text>
      </g>)}
      {edges.map(({ from, to }) => {
        const down = to.y > from.y, same = to.y === from.y;
        const x1 = from.x + from.w / 2, x2 = to.x + to.w / 2;
        const y1 = same ? from.y : down ? from.y + BOX_H : from.y, y2 = same ? to.y : down ? to.y : to.y + BOX_H;
        const bend = same ? -30 : (y2 - y1) / 2;
        return <path key={`${from.id}-${to.id}`} d={`M${x1},${y1} C${x1},${y1 + bend} ${x2},${y2 - (same ? -bend : bend)} ${x2},${y2}`} className="learn-diagram-edge" markerEnd="url(#learn-arrow)" />;
      })}
      {[...boxes.values()].map(box => {
        const pageId = `module:${box.id}`;
        return <g key={box.id} className="learn-diagram-node" data-level={box.level} data-ready={readyPages.has(pageId) || undefined}
          role="button" tabIndex={0} aria-label={box.title} onClick={() => onOpen(pageId)}
          onKeyDown={event => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); onOpen(pageId); } }}>
          <rect x={box.x} y={box.y} width={box.w} height={BOX_H} rx={8} />
          <text x={box.x + box.w / 2} y={box.y + 19} textAnchor="middle" className="learn-diagram-node-title">{clip(box.title, Math.floor(box.w / 7.5))}</text>
          <text x={box.x + box.w / 2} y={box.y + 34} textAnchor="middle" className="learn-diagram-node-level">L{box.level}</text>
          <title>{box.title}</title>
        </g>;
      })}
    </svg>
    <figcaption>{t("learnDiagramCaption")}</figcaption>
  </figure>;
}

function clip(text: string, max: number) {
  return text.length > max ? `${text.slice(0, Math.max(max - 1, 1))}…` : text;
}
