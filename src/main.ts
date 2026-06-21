import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { Avatar } from "./avatar";
import { renderBoard, resizeBoard, type DraftBoard } from "./graph";

interface Segment {
  id: string;
  text: string;
  edited: boolean;
}
interface Session {
  id: string;
  title: string;
  created_at: string;
  segments: Segment[];
}
interface SessionMeta {
  id: string;
  title: string;
  created_at: string;
  segment_count: number;
}

const $ = <T extends HTMLElement>(sel: string) => document.querySelector<T>(sel)!;

const segmentsEl = $<HTMLDivElement>("#segments");
const liveEl = $<HTMLDivElement>("#live");
const transcriptEl = $<HTMLElement>("#transcript");
const stateLabel = $<HTMLSpanElement>("#state-label");
const recBtn = $<HTMLButtonElement>("#rec");
const hintEl = $<HTMLParagraphElement>("#hint");
const titleEl = $<HTMLHeadingElement>("#session-title");
const listEl = $<HTMLUListElement>("#session-list");
const newBtn = $<HTMLButtonElement>("#new-session");

let currentId = "";

// ---------- avatar ----------
const avatar = new Avatar($<HTMLCanvasElement>("#avatar-canvas"));
avatar.start();
avatar.loadVRM("/avatar.vrm").then(() => avatar.resize()); // procedural fallback if missing
$<HTMLDivElement>("#avatar").addEventListener("click", () => invoke("toggle_recording"));
window.addEventListener("resize", () => avatar.resize());

// ---------- helpers ----------
let autoScroll = true;
transcriptEl.addEventListener("scroll", () => {
  autoScroll =
    transcriptEl.scrollHeight - transcriptEl.scrollTop - transcriptEl.clientHeight < 48;
});
function keepBottom() {
  if (autoScroll) transcriptEl.scrollTop = transcriptEl.scrollHeight;
}
function escapeHtml(s: string): string {
  return s.replace(/[&<>]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[c]!);
}
function autoGrow(ta: HTMLTextAreaElement) {
  ta.style.height = "auto";
  ta.style.height = ta.scrollHeight + "px";
}

// debounce per segment id
const saveTimers = new Map<string, number>();
function scheduleSave(segId: string, text: string) {
  const prev = saveTimers.get(segId);
  if (prev) clearTimeout(prev);
  saveTimers.set(
    segId,
    window.setTimeout(() => {
      invoke("update_segment", { sessionId: currentId, segId, text }).catch(() => {});
    }, 500),
  );
}

// ---------- rendering ----------
function renderSegment(seg: Segment) {
  hintEl.style.display = "none";
  const wrap = document.createElement("div");
  wrap.className = "segment";

  const ta = document.createElement("textarea");
  ta.value = seg.text;
  ta.spellcheck = false;
  ta.rows = 1;
  ta.addEventListener("input", () => {
    autoGrow(ta);
    scheduleSave(seg.id, ta.value);
  });

  const copy = document.createElement("button");
  copy.className = "seg-copy";
  copy.title = "Sao chép";
  copy.textContent = "⧉";
  copy.addEventListener("click", () => {
    navigator.clipboard.writeText(ta.value).then(() => {
      copy.textContent = "✓";
      setTimeout(() => (copy.textContent = "⧉"), 1000);
    });
  });

  wrap.appendChild(ta);
  wrap.appendChild(copy);
  segmentsEl.appendChild(wrap);
  autoGrow(ta);
}

function renderSession(s: Session) {
  currentId = s.id;
  titleEl.textContent = s.title || `Phiên ${s.created_at}`;
  segmentsEl.innerHTML = "";
  liveEl.innerHTML = "";
  hintEl.style.display = s.segments.length ? "none" : "block";
  for (const seg of s.segments) renderSegment(seg);
  keepBottom();
}

async function refreshList() {
  const sessions = await invoke<SessionMeta[]>("list_sessions");
  listEl.innerHTML = "";
  for (const m of sessions) {
    const li = document.createElement("li");
    li.className = "session-item" + (m.id === currentId ? " active" : "");
    li.innerHTML =
      `<span class="s-title">${escapeHtml(m.title)}</span>` +
      `<span class="s-meta">${escapeHtml(m.created_at)} · ${m.segment_count} đoạn</span>`;
    li.addEventListener("click", () => selectSession(m.id));
    listEl.appendChild(li);
  }
}

async function selectSession(id: string) {
  if (id === currentId) return;
  const s = await invoke<Session>("switch_session", { id });
  renderSession(s);
  refreshList();
}

// ---------- events from backend ----------
listen<{ committed: string; preview: string }>("transcript:update", (e) => {
  const { committed, preview } = e.payload;
  liveEl.innerHTML =
    `<span class="committed">${escapeHtml(committed)}</span>` +
    `<span class="preview">${escapeHtml(preview)}</span>`;
  keepBottom();
});

listen("transcript:final", () => {
  liveEl.innerHTML = "";
});

listen<Segment>("session:segment", (e) => {
  renderSegment(e.payload);
  keepBottom();
  refreshList();
});

listen<string>("state:changed", (e) => {
  const recording = e.payload === "listening";
  document.body.dataset.state = e.payload;
  stateLabel.textContent = recording ? "Đang nghe…" : "Sẵn sàng";
  recBtn.textContent = recording ? "■ Dừng (F7)" : "● Ghi (F7)";
  avatar.setState(recording ? "listening" : "idle");
});

listen<number>("audio:level", (e) => {
  avatar.setLevel(e.payload);
});

listen<string>("error", (e) => {
  stateLabel.textContent = "Lỗi: " + e.payload;
});

// ---------- UI actions ----------
recBtn.addEventListener("click", () => invoke("toggle_recording"));

newBtn.addEventListener("click", async () => {
  const meta = await invoke<SessionMeta>("new_session");
  renderSession({ id: meta.id, title: meta.title, created_at: meta.created_at, segments: [] });
  refreshList();
});

titleEl.addEventListener("blur", () => {
  const title = (titleEl.textContent || "").trim();
  if (currentId) invoke("set_session_title", { sessionId: currentId, title }).then(refreshList);
});
titleEl.addEventListener("keydown", (e) => {
  if (e.key === "Enter") {
    e.preventDefault();
    titleEl.blur();
  }
});

// ---------- draft board ----------
const draftBtn = $<HTMLButtonElement>("#draft-btn");
const boardModal = $<HTMLDivElement>("#board-modal");
const boardSummary = $<HTMLSpanElement>("#board-summary");
const boardGraph = $<HTMLDivElement>("#board-graph");

function restoreAvatar() {
  avatar.start();
  avatar.setState(document.body.dataset.state === "listening" ? "listening" : "idle");
}

$<HTMLButtonElement>("#board-close").addEventListener("click", () => {
  boardModal.hidden = true;
  restoreAvatar();
});
window.addEventListener("resize", () => {
  if (!boardModal.hidden) resizeBoard(boardGraph);
});

draftBtn.addEventListener("click", async () => {
  draftBtn.disabled = true;
  const prev = stateLabel.textContent;
  stateLabel.textContent = "Đang vẽ sơ đồ ý tưởng…";
  avatar.setState("thinking");
  try {
    const board = await invoke<DraftBoard>("generate_draft_board");
    boardSummary.textContent = board.summary || "";
    boardModal.hidden = false;
    avatar.pause(); // board covers the screen — free the GPU for the graph
    // Defer to next frame so the modal has laid out (canvas gets a real size).
    requestAnimationFrame(() => renderBoard(boardGraph, board));
    stateLabel.textContent = prev || "Sẵn sàng";
  } catch (err) {
    stateLabel.textContent = "Draft board lỗi: " + err;
    avatar.setState(document.body.dataset.state === "listening" ? "listening" : "idle");
  } finally {
    draftBtn.disabled = false;
  }
});

// ---------- assistant reply (Claude + Soniox TTS) ----------
const replyBtn = $<HTMLButtonElement>("#reply-btn");
let audioCtx: AudioContext | null = null;

function addReplyBlock(text: string) {
  hintEl.style.display = "none";
  const div = document.createElement("div");
  div.className = "reply-block";
  div.textContent = "🤖 " + text;
  segmentsEl.appendChild(div);
  keepBottom();
}

async function speakAudio(b64: string): Promise<void> {
  audioCtx ??= new AudioContext();
  if (audioCtx.state === "suspended") await audioCtx.resume();
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const buf = await audioCtx.decodeAudioData(bytes.buffer);

  const src = audioCtx.createBufferSource();
  src.buffer = buf;
  const analyser = audioCtx.createAnalyser();
  analyser.fftSize = 256;
  src.connect(analyser);
  analyser.connect(audioCtx.destination);
  const data = new Uint8Array(analyser.frequencyBinCount);

  avatar.setState("speaking");
  let raf = 0;
  const tick = () => {
    analyser.getByteFrequencyData(data);
    let s = 0;
    for (let i = 0; i < data.length; i++) s += data[i];
    avatar.setLevel(Math.min(1, s / data.length / 80)); // drive the VRM mouth
    raf = requestAnimationFrame(tick);
  };
  return new Promise((resolve) => {
    src.onended = () => {
      cancelAnimationFrame(raf);
      avatar.setLevel(0);
      avatar.setState(document.body.dataset.state === "listening" ? "listening" : "idle");
      resolve();
    };
    src.start();
    tick();
  });
}

replyBtn.addEventListener("click", async () => {
  replyBtn.disabled = true;
  const prev = stateLabel.textContent;
  stateLabel.textContent = "Trợ lý đang nghĩ…";
  avatar.setState("thinking");
  try {
    const r = await invoke<{ text: string; audio_b64: string }>("speak_reply");
    addReplyBlock(r.text);
    stateLabel.textContent = prev || "Sẵn sàng";
    await speakAudio(r.audio_b64);
  } catch (err) {
    stateLabel.textContent = "Trợ lý lỗi: " + err;
    avatar.setState(document.body.dataset.state === "listening" ? "listening" : "idle");
  } finally {
    replyBtn.disabled = false;
  }
});

// ---------- boot ----------
(async () => {
  const s = await invoke<Session>("current_session");
  renderSession(s);
  refreshList();
})();
