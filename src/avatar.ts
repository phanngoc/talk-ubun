import * as THREE from "three";
import { GLTFLoader } from "three/addons/loaders/GLTFLoader.js";
import {
  VRMLoaderPlugin,
  VRMUtils,
  VRMExpressionPresetName,
  type VRM,
} from "@pixiv/three-vrm";

export type AvatarState = "idle" | "listening" | "thinking" | "speaking";

const CYAN = new THREE.Color(0x3fe0cb);
const VIOLET = new THREE.Color(0xa98bff);

/**
 * The on-screen assistant. Loads a VRM character if `/avatar.vrm` is present and
 * drives amplitude-based lip-sync from the mic level; otherwise falls back to a
 * lightweight procedural "hologram" (wireframe core + particle shell + glow).
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
  private running = false;

  private vrm: VRM | null = null;
  private blinkT = 0;

  constructor(canvas: HTMLCanvasElement) {
    this.canvas = canvas;
    this.renderer = new THREE.WebGLRenderer({ canvas, alpha: true, antialias: true });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.5));

    this.camera = new THREE.PerspectiveCamera(50, 1, 0.1, 100);
    this.camera.position.set(0, 0, 4);

    // Lights (for the VRM's MToon materials; the procedural lines ignore them).
    this.scene.add(new THREE.AmbientLight(0xffffff, 1.6));
    const dir = new THREE.DirectionalLight(0xffffff, 2.0);
    dir.position.set(1, 1.5, 1.2);
    this.scene.add(dir);

    // Procedural core.
    const wire = new THREE.WireframeGeometry(new THREE.IcosahedronGeometry(1, 1));
    this.coreMat = new THREE.LineBasicMaterial({
      color: this.color.clone(),
      transparent: true,
      opacity: 0.8,
    });
    this.core = new THREE.LineSegments(wire, this.coreMat);
    this.scene.add(this.core);

    // Particle shell.
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

  /** Try to load a VRM; on success, hide the procedural avatar and frame the head. */
  async loadVRM(url: string) {
    const loader = new GLTFLoader();
    loader.register((parser) => new VRMLoaderPlugin(parser));
    try {
      const gltf = await loader.loadAsync(url);
      const vrm = gltf.userData.vrm as VRM | undefined;
      if (!vrm) throw new Error("no VRM in file");

      VRMUtils.rotateVRM0(vrm); // face +Z (no-op for VRM1)
      VRMUtils.combineSkeletons(gltf.scene);
      vrm.scene.traverse((o) => (o.frustumCulled = false));
      this.scene.add(vrm.scene);
      this.vrm = vrm;

      // Frame the head in the small canvas.
      vrm.scene.updateMatrixWorld(true);
      const head = vrm.humanoid?.getNormalizedBoneNode("head");
      const hp = new THREE.Vector3(0, 1.35, 0);
      head?.getWorldPosition(hp);
      this.camera.position.set(hp.x, hp.y + 0.05, hp.z + 0.62);
      this.camera.lookAt(hp.x, hp.y + 0.05, hp.z);
      this.glow.position.set(hp.x, hp.y, hp.z - 0.2);
      this.glow.scale.setScalar(1.1);

      // Hide procedural visuals.
      this.core.visible = false;
      this.particles.visible = false;
    } catch (e) {
      console.warn("VRM load failed, using procedural avatar:", e);
    }
  }

  setLevel(v: number) {
    this.target = Math.min(1, Math.max(0, v));
  }

  setState(s: AvatarState) {
    this.state = s;
    this.targetColor = s === "thinking" ? VIOLET.clone() : CYAN.clone();
  }

  resize() {
    const w = this.canvas.clientWidth || 152;
    const h = this.canvas.clientHeight || 152;
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
  }

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
    this.glowMat.color.copy(this.color);

    if (this.vrm) {
      const e = this.vrm.expressionManager;
      if (e) {
        e.setValue(VRMExpressionPresetName.Aa, Math.min(1, this.level * 1.7));
        this.blinkT += dt;
        e.setValue(VRMExpressionPresetName.Blink, this.blinkT % 4 < 0.12 ? 1 : 0);
      }
      const head = this.vrm.humanoid?.getNormalizedBoneNode("head");
      if (head) {
        head.rotation.y = Math.sin(t * 0.7) * 0.07;
        head.rotation.x = Math.sin(t * 0.9) * 0.03 + this.level * 0.04;
      }
      this.vrm.update(dt);
      this.glow.scale.setScalar(0.9 + this.level * 0.8);
      this.glowMat.opacity = 0.12 + this.level * 0.4;
    } else {
      this.coreMat.color.copy(this.color);
      this.partMat.color.copy(this.color);
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
    }

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
