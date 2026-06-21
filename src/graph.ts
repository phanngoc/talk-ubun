import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import SpriteText from "three-spritetext";

export interface DraftPlot {
  label: string;
  expr: string;
  dim: number; // 2 = y=f(x), 3 = z=f(x,y) surface
  xmin: number;
  xmax: number;
}
export interface DraftBoard {
  summary: string;
  plots: DraftPlot[];
}

const CYAN = 0x3fe0cb;
const AXIS = 0x4a6470;

const plots: DraftPlot[] = [];
const exprs = new Set<string>();
let summary = "";

let renderer: THREE.WebGLRenderer | null = null;
let scene: THREE.Scene;
let camera: THREE.PerspectiveCamera;
let controls: OrbitControls;
let group: THREE.Group;
let started = false;

function ensure(container: HTMLElement) {
  if (started) return;
  started = true;

  renderer = new THREE.WebGLRenderer({ alpha: true, antialias: true });
  renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.5));
  renderer.domElement.style.cssText = "width:100%;height:100%;display:block";
  container.appendChild(renderer.domElement);

  scene = new THREE.Scene();
  camera = new THREE.PerspectiveCamera(50, 1, 0.01, 1000);
  camera.position.set(2.4, 1.8, 3.6);

  controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.autoRotate = true;
  controls.autoRotateSpeed = 0.7;

  group = new THREE.Group();
  scene.add(group);

  const loop = () => {
    requestAnimationFrame(loop);
    controls.update();
    renderer!.render(scene, camera);
  };
  loop();
  resizeBoard(container);
}

// --- math expression compilers (whitelisted chars only) ---
function compile(expr: string, vars: string): ((...a: number[]) => number) | null {
  if (!/^[0-9a-zA-Z_.+\-*/(),\s^]*$/.test(expr)) return null;
  const js = expr.replace(/\^/g, "**");
  try {
    return new Function(...vars.split(","), `with (Math) { return (${js}); }`) as (
      ...a: number[]
    ) => number;
  } catch {
    return null;
  }
}

function disposeGroup() {
  group.traverse((o: any) => {
    if (o.geometry) o.geometry.dispose();
    if (o.material) {
      const m = o.material;
      if (Array.isArray(m)) m.forEach((x) => x.dispose());
      else m.dispose();
    }
  });
  group.clear();
}

function lineMat(color: number, opacity = 1) {
  return new THREE.LineBasicMaterial({ color, transparent: opacity < 1, opacity });
}

function axes2d(): THREE.Group {
  const g = new THREE.Group();
  const mk = (a: THREE.Vector3, b: THREE.Vector3) =>
    new THREE.Line(new THREE.BufferGeometry().setFromPoints([a, b]), lineMat(AXIS, 0.6));
  g.add(mk(new THREE.Vector3(-1.1, 0, 0), new THREE.Vector3(1.1, 0, 0)));
  g.add(mk(new THREE.Vector3(0, -1.1, 0), new THREE.Vector3(0, 1.1, 0)));
  return g;
}

function buildPlot(p: DraftPlot): THREE.Object3D | null {
  const xmin = Number.isFinite(p.xmin) ? p.xmin : -6.283;
  const xmax = Number.isFinite(p.xmax) && p.xmax > xmin ? p.xmax : xmin + 12.566;
  const g = new THREE.Group();

  if (p.dim === 3) {
    const f = compile(p.expr, "x,y");
    if (!f) return null;
    const G = 36;
    const zs: number[] = [];
    let zmax = 1e-6;
    for (let j = 0; j <= G; j++) {
      for (let i = 0; i <= G; i++) {
        const x = xmin + ((xmax - xmin) * i) / G;
        const y = xmin + ((xmax - xmin) * j) / G;
        let z = f(x, y);
        if (!Number.isFinite(z)) z = 0;
        zs.push(z);
        zmax = Math.max(zmax, Math.abs(z));
      }
    }
    const pos: number[] = [];
    for (let j = 0; j <= G; j++) {
      for (let i = 0; i <= G; i++) {
        pos.push((i / G) * 2 - 1, zs[j * (G + 1) + i] / zmax, (j / G) * 2 - 1);
      }
    }
    const idx: number[] = [];
    for (let j = 0; j < G; j++) {
      for (let i = 0; i < G; i++) {
        const a = j * (G + 1) + i;
        idx.push(a, a + 1, a + G + 2, a, a + G + 2, a + G + 1);
      }
    }
    const geom = new THREE.BufferGeometry();
    geom.setAttribute("position", new THREE.Float32BufferAttribute(pos, 3));
    geom.setIndex(idx);
    geom.computeVertexNormals();
    g.add(
      new THREE.Mesh(
        geom,
        new THREE.MeshBasicMaterial({ color: CYAN, wireframe: true, transparent: true, opacity: 0.85 }),
      ),
    );
  } else {
    const f = compile(p.expr, "x");
    if (!f) return null;
    const N = 200;
    const ys: number[] = [];
    let ymax = 1e-6;
    for (let i = 0; i <= N; i++) {
      const x = xmin + ((xmax - xmin) * i) / N;
      const y = f(x);
      ys.push(y);
      if (Number.isFinite(y)) ymax = Math.max(ymax, Math.abs(y));
    }
    const pts: THREE.Vector3[] = [];
    for (let i = 0; i <= N; i++) {
      if (!Number.isFinite(ys[i])) continue;
      pts.push(new THREE.Vector3((i / N) * 2 - 1, ys[i] / ymax, 0));
    }
    g.add(axes2d());
    g.add(new THREE.Line(new THREE.BufferGeometry().setFromPoints(pts), lineMat(CYAN)));
  }

  const label = new SpriteText(p.label);
  label.color = "#9fe9dd";
  label.textHeight = 0.16;
  label.position.set(0, 1.35, 0);
  g.add(label);
  return g;
}

function rebuild() {
  disposeGroup();
  const SPAN = 3;
  const objs: THREE.Object3D[] = [];
  plots.forEach((p) => {
    const o = buildPlot(p);
    if (o) objs.push(o);
  });
  objs.forEach((o, i) => {
    o.position.x = (i - (objs.length - 1) / 2) * SPAN;
    group.add(o);
  });
  fitCamera();
}

function fitCamera() {
  if (!group.children.length) return;
  const box = new THREE.Box3().setFromObject(group);
  const center = box.getCenter(new THREE.Vector3());
  const size = box.getSize(new THREE.Vector3());
  const r = Math.max(size.x, size.y, size.z, 1) * 0.5;
  controls.target.copy(center);
  camera.position.set(center.x + r * 1.6, center.y + r * 1.3, center.z + r * 2.4);
  camera.near = r / 100;
  camera.far = r * 100;
  camera.updateProjectionMatrix();
  controls.update();
}

export function applyDelta(container: HTMLElement, delta: DraftBoard) {
  ensure(container);
  for (const p of delta.plots ?? []) {
    const k = (p.expr || "").trim();
    if (k && !exprs.has(k)) {
      exprs.add(k);
      plots.push(p);
    }
  }
  if (delta.summary && delta.summary.trim()) summary = delta.summary;
  rebuild();
  resizeBoard(container);
}

export function resetBoard() {
  plots.length = 0;
  exprs.clear();
  summary = "";
  if (started) {
    disposeGroup();
  }
}

export function boardSnapshot(): DraftBoard {
  return { summary, plots: plots.map((p) => ({ ...p })) };
}
export function getSummary() {
  return summary;
}
export function boardEmpty() {
  return plots.length === 0;
}

export function resizeBoard(container: HTMLElement) {
  if (!renderer || !camera) return;
  const w = container.clientWidth || 480;
  const h = container.clientHeight || 360;
  renderer.setSize(w, h, false);
  camera.aspect = w / h;
  camera.updateProjectionMatrix();
}
