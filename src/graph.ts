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

let graph: any = null;

export function renderBoard(container: HTMLElement, board: DraftBoard) {
  const nodes = board.nodes.map((n) => ({ ...n }));
  const links = board.edges.map((e) => ({
    source: e.from,
    target: e.to,
    relation: e.relation,
  }));

  if (!graph) {
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
      .linkDirectionalParticleSpeed(0.006);
  }

  graph.graphData({ nodes, links });
  resizeBoard(container);
  // Re-size once the modal has fully laid out, then frame the nodes. Without
  // this the canvas can come up 0×0 (only the nav hint shows) or off-camera.
  setTimeout(() => {
    resizeBoard(container);
    try {
      graph.zoomToFit(600, 60);
    } catch {
      /* no nodes yet */
    }
  }, 450);
}

export function resizeBoard(container: HTMLElement) {
  if (graph) {
    graph.width(container.clientWidth || window.innerWidth);
    graph.height(container.clientHeight || window.innerHeight - 60);
  }
}
