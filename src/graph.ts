import ForceGraph3D from "3d-force-graph";
import SpriteText from "three-spritetext";

export interface DraftNode {
  id: string;
  label: string;
  kind: string;
  note: string;
}
export interface DraftEdge {
  from: string;
  to: string;
  relation: string;
}
export interface DraftBoard {
  summary: string;
  nodes: DraftNode[];
  edges: DraftEdge[];
}

const KIND_COLORS: Record<string, string> = {
  idea: "#3fe0cb",
  feature: "#5ad1ff",
  task: "#a98bff",
  entity: "#ffd166",
  question: "#ff8fa3",
  decision: "#7CFFB2",
};
const colorFor = (kind: string) => KIND_COLORS[kind] ?? "#3fe0cb";

// Live board state. Node objects are reused across updates so 3d-force-graph
// keeps their positions when new nodes are merged in (the "increment" feel).
let graph: any = null;
const gnodes: any[] = [];
const glinks: any[] = [];
const nodeIds = new Set<string>();
let summary = "";

function ensureGraph(container: HTMLElement) {
  if (graph) return;
  graph = new (ForceGraph3D as any)(container)
    .backgroundColor("rgba(7,11,20,0)")
    .nodeRelSize(5)
    .nodeColor((n: any) => colorFor(n.kind))
    .nodeLabel((n: any) => `<b>${n.label}</b><br/><span>${n.note ?? ""}</span>`)
    .nodeThreeObjectExtend(true)
    .nodeThreeObject((n: any) => {
      const s = new SpriteText(n.label);
      s.color = "#dceaee";
      s.textHeight = 5;
      s.backgroundColor = "rgba(7,11,20,0.55)";
      s.padding = 2;
      s.position.set(0, 9, 0);
      return s;
    })
    .linkColor(() => "rgba(63,224,203,0.35)")
    .linkLabel((l: any) => l.relation)
    .linkWidth(0.6)
    .linkDirectionalArrowLength(3.5)
    .linkDirectionalArrowRelPos(1)
    .linkDirectionalParticles(2)
    .linkDirectionalParticleSpeed(0.006)
    .cooldownTicks(150)
    .onEngineStop(() => {
      resizeBoard(container);
      try {
        graph.zoomToFit(500, 50);
      } catch {
        /* no nodes */
      }
    });
  graph.d3Force("charge")?.strength(-180); // push nodes apart so they don't stack
}

export function resetBoard() {
  gnodes.length = 0;
  glinks.length = 0;
  nodeIds.clear();
  summary = "";
  if (graph) graph.graphData({ nodes: [], links: [] });
}

/** Merge a delta (new nodes/edges) into the live board. */
export function applyDelta(container: HTMLElement, delta: DraftBoard) {
  ensureGraph(container);
  for (const n of delta.nodes ?? []) {
    if (n.id && !nodeIds.has(n.id)) {
      nodeIds.add(n.id);
      gnodes.push({ id: n.id, label: n.label, kind: n.kind, note: n.note });
    }
  }
  for (const e of delta.edges ?? []) {
    if (e.from && e.to && nodeIds.has(e.from) && nodeIds.has(e.to)) {
      glinks.push({ source: e.from, target: e.to, relation: e.relation });
    }
  }
  if (delta.summary && delta.summary.trim()) summary = delta.summary;
  graph.graphData({ nodes: gnodes, links: glinks });
  resizeBoard(container);
}

/** Compact board to send back to the backend (drop notes to save tokens). */
export function boardSnapshot(): DraftBoard {
  return {
    summary,
    nodes: gnodes.map((n) => ({ id: n.id, label: n.label, kind: n.kind, note: "" })),
    edges: glinks.map((l) => ({
      from: l.source?.id ?? l.source,
      to: l.target?.id ?? l.target,
      relation: l.relation,
    })),
  };
}

export function getSummary() {
  return summary;
}
export function nodeCount() {
  return gnodes.length;
}

export function resizeBoard(container: HTMLElement) {
  if (graph) {
    graph.width(container.clientWidth || window.innerWidth);
    graph.height(container.clientHeight || window.innerHeight - 60);
  }
}
