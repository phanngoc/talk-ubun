import * as THREE from "three";

export type AvatarState = "idle" | "listening" | "thinking" | "speaking";

const CYAN = new THREE.Color(0x3fe0cb);
const VIOLET = new THREE.Color(0xa98bff);

/**
 * A lightweight procedural "hologram" avatar: a rotating wireframe core inside a
 * particle shell with a glow, all reacting to the mic level. A stand-in for a
 * full VRM character (drop-in later) that keeps WebKitGTK happy.
 */
export class Avatar {
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera: THREE.PerspectiveCamera;
  private core: THREE.LineSegments;
  private coreMat: THREE.LineBasicMaterial;
  private particles: THREE.Points;
  private partMat: THREE.PointsMaterial;
  private glow: THREE.Sprite;
  private glowMat: THREE.SpriteMaterial;
  private clock = new THREE.Clock();
  private level = 0;
  private target = 0;
  private state: AvatarState = "idle";
  private color = CYAN.clone();
  private targetColor = CYAN.clone();
  private canvas: HTMLCanvasElement;

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;
    this.renderer = new THREE.WebGLRenderer({ canvas, alpha: true, antialias: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.5));

    this.camera = new THREE.PerspectiveCamera(50, 1, 0.1, 100);
    this.camera.position.set(0, 0, 4);

    // Wireframe core.
    const wire = new THREE.WireframeGeometry(new THREE.IcosahedronGeometry(1, 1));
    this.coreMat = new THREE.LineBasicMaterial({
      color: this.color.clone(),
      transparent: true,
      opacity: 0.8,
    });
    this.core = new THREE.LineSegments(wire, this.coreMat);
    this.scene.add(this.core);

    // Particle shell (fibonacci sphere with jitter).
    const N = 900;
    const positions = new Float32Array(N * 3);
    const golden = Math.PI * (3 - Math.sqrt(5));
    for (let i = 0; i < N; i++) {
      const y = 1 - (i / (N - 1)) * 2;
      const r = Math.sqrt(1 - y * y);
      const th = golden * i;
      const rad = 1.5 + Math.random() * 0.3;
      positions[i * 3] = Math.cos(th) * r * rad;
      positions[i * 3 + 1] = y * rad;
      positions[i * 3 + 2] = Math.sin(th) * r * rad;
    }
    const pgeo = new THREE.BufferGeometry();
    pgeo.setAttribute("position", new THREE.BufferAttribute(positions, 3));
    this.partMat = new THREE.PointsMaterial({
      color: this.color.clone(),
      size: 0.03,
      transparent: true,
      opacity: 0.7,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
    });
    this.particles = new THREE.Points(pgeo, this.partMat);
    this.scene.add(this.particles);

    // Glow.
    this.glowMat = new THREE.SpriteMaterial({
      map: makeGlowTexture(),
      color: this.color.clone(),
      transparent: true,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      opacity: 0.22,
    });
    this.glow = new THREE.Sprite(this.glowMat);
    this.glow.scale.setScalar(3);
    this.scene.add(this.glow);

    this.resize();
  }

  setLevel(v: number) {
    this.target = Math.min(1, Math.max(0, v));
  }

  setState(s: AvatarState) {
    this.state = s;
    this.targetColor = s === "thinking" ? VIOLET.clone() : CYAN.clone();
  }

  resize() {
    const w = this.canvas.clientWidth || 150;
    const h = this.canvas.clientHeight || 150;
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
  }

  private running = false;

  start() {
    if (this.running) return;
    this.running = true;
    const loop = () => {
      if (!this.running) return;
      requestAnimationFrame(loop);
      this.frame();
    };
    loop();
  }

  pause() {
    this.running = false;
  }

  private frame() {
    const dt = Math.min(this.clock.getDelta(), 0.05);
    const t = this.clock.elapsedTime;

    this.level += (this.target - this.level) * Math.min(1, dt * 12);
    this.color.lerp(this.targetColor, Math.min(1, dt * 4));
    this.coreMat.color.copy(this.color);
    this.partMat.color.copy(this.color);
    this.glowMat.color.copy(this.color);

    const spin = this.state === "thinking" ? 1.4 : 0.35;
    this.core.rotation.y += dt * spin;
    this.core.rotation.x += dt * spin * 0.4;
    this.particles.rotation.y -= dt * spin * 0.5;

    const breathe = Math.sin(t * 1.6) * 0.03;
    this.core.scale.setScalar(1 + breathe + this.level * 0.6);
    this.particles.scale.setScalar(1 + this.level * 0.25);
    this.partMat.size = 0.03 + this.level * 0.05;
    this.coreMat.opacity = 0.55 + this.level * 0.45;
    this.glow.scale.setScalar(2.6 + this.level * 2.2);
    this.glowMat.opacity = 0.18 + this.level * 0.5;

    this.renderer.render(this.scene, this.camera);
  }
}

function makeGlowTexture(): THREE.Texture {
  const c = document.createElement("canvas");
  c.width = c.height = 128;
  const ctx = c.getContext("2d")!;
  const g = ctx.createRadialGradient(64, 64, 0, 64, 64, 64);
  g.addColorStop(0, "rgba(255,255,255,0.9)");
  g.addColorStop(0.3, "rgba(255,255,255,0.35)");
  g.addColorStop(1, "rgba(255,255,255,0)");
  ctx.fillStyle = g;
  ctx.fillRect(0, 0, 128, 128);
  return new THREE.CanvasTexture(c);
}
