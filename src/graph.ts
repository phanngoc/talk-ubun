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
export interface DraftPlot {
  label: string;
  expr: string;
  xmin: number;
  xmax: number;
}
export interface DraftBoard {
  summary: string;
  nodes: DraftNode[];
  edges: DraftEdge[];
  plots?: DraftPlot[];
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
const gplots: DraftPlot[] = [];
const plotExprs = new Set<string>();
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
  gplots.length = 0;
  plotExprs.clear();
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
  for (const p of delta.plots ?? []) {
    const key = (p.expr || "").trim();
    if (key && !plotExprs.has(key)) {
      plotExprs.add(key);
      gplots.push(p);
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

export function plotCount() {
  return gplots.length;
}

// Compile a Claude-provided JS math expression in terms of x (e.g. "Math.sin(x)").
function compile(expr: string): ((x: number) => number) | null {
  if (!/^[0-9a-zA-Z_.+\-*/(),\s^]*$/.test(expr)) return null; // reject anything but math
  const js = expr.replace(/\^/g, "**");
  try {
    return new Function("x", `with (Math) { return (${js}); }`) as (x: number) => number;
  } catch {
    return null;
  }
}

// Draw all accumulated function plots as 2D curves on a canvas (clear for math).
export function drawPlots(canvas: HTMLCanvasElement) {
  const ctx = canvas.getContext("2d");
  if (!ctx) return;
  const dpr = Math.min(window.devicePixelRatio, 2);
  const W = canvas.clientWidth || 360;
  const H = canvas.clientHeight || 220;
  canvas.width = Math.round(W * dpr);
  canvas.height = Math.round(H * dpr);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, W, H);
  if (!gplots.length) return;

  let xmin = Infinity;
  let xmax = -Infinity;
  for (const p of gplots) {
    xmin = Math.min(xmin, p.xmin);
    xmax = Math.max(xmax, p.xmax);
  }
  if (!(xmax > xmin)) {
    xmin = -6.283;
    xmax = 6.283;
  }

  const N = 240;
  let ymin = Infinity;
  let ymax = -Infinity;
  const series = gplots.map((p) => {
    const f = compile(p.expr);
    const pts: [number, number][] = [];
    if (f) {
      for (let i = 0; i <= N; i++) {
        const x = xmin + ((xmax - xmin) * i) / N;
        const y = f(x);
        if (Number.isFinite(y)) {
          pts.push([x, y]);
          ymin = Math.min(ymin, y);
          ymax = Math.max(ymax, y);
        }
      }
    }
    return { p, pts };
  });
  if (!(ymax > ymin)) {
    ymin = -1;
    ymax = 1;
  }
  const yr = ymax - ymin;
  ymin -= yr * 0.08;
  ymax += yr * 0.08;

  const pad = 24;
  const X = (x: number) => pad + ((x - xmin) / (xmax - xmin)) * (W - 2 * pad);
  const Y = (y: number) => H - pad - ((y - ymin) / (ymax - ymin)) * (H - 2 * pad);

  ctx.strokeStyle = "rgba(124,147,160,0.4)";
  ctx.lineWidth = 1;
  ctx.beginPath();
  if (0 >= ymin && 0 <= ymax) {
    ctx.moveTo(pad, Y(0));
    ctx.lineTo(W - pad, Y(0));
  }
  if (0 >= xmin && 0 <= xmax) {
    ctx.moveTo(X(0), pad);
    ctx.lineTo(X(0), H - pad);
  }
  ctx.stroke();

  const colors = ["#3fe0cb", "#a98bff", "#ffd166", "#5ad1ff", "#ff8fa3"];
  series.forEach((s, idx) => {
    const col = colors[idx % colors.length];
    ctx.strokeStyle = col;
    ctx.lineWidth = 2;
    ctx.beginPath();
    s.pts.forEach(([x, y], i) => {
      const px = X(x);
      const py = Y(y);
      if (i) ctx.lineTo(px, py);
      else ctx.moveTo(px, py);
    });
    ctx.stroke();
    ctx.fillStyle = col;
    ctx.font = "12px ui-monospace, monospace";
    ctx.fillText(s.p.label, pad + 6, pad + 12 + idx * 15);
  });
}
