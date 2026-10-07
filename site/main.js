"use strict";

const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

/* Theme: follows the system until the toggle is used, then remembers the choice. */
function currentTheme() {
  const set = document.documentElement.dataset.theme;
  if (set) return set;
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

document.getElementById("theme-toggle").addEventListener("click", () => {
  const next = currentTheme() === "dark" ? "light" : "dark";
  document.documentElement.dataset.theme = next;
  try {
    localStorage.setItem("lotml-theme", next);
  } catch (e) {
    /* storage blocked: the choice lasts for this page only */
  }
});

/* Tabs: every [data-tabs] holds role="tab" buttons that show one role="tabpanel". */
for (const group of document.querySelectorAll("[data-tabs]")) {
  const tabs = [...group.querySelectorAll('[role="tab"]')];
  const select = (tab) => {
    for (const t of tabs) {
      const on = t === tab;
      t.setAttribute("aria-selected", String(on));
      t.tabIndex = on ? 0 : -1;
      document.getElementById(t.getAttribute("aria-controls")).hidden = !on;
    }
  };
  tabs.forEach((tab, i) => {
    tab.tabIndex = tab.getAttribute("aria-selected") === "true" ? 0 : -1;
    tab.addEventListener("click", () => select(tab));
    tab.addEventListener("keydown", (e) => {
      const step = { ArrowRight: 1, ArrowLeft: -1 }[e.key];
      if (!step) return;
      const next = tabs[(i + step + tabs.length) % tabs.length];
      select(next);
      next.focus();
    });
  });
}

/* Copy: the visible panel's text, without the comments after `#`. */
for (const button of document.querySelectorAll("[data-copy]")) {
  button.addEventListener("click", async () => {
    const panel = button.closest(".window").querySelector('[role="tabpanel"]:not([hidden]) code');
    const text = panel.textContent
      .split("\n")
      .map((line) => line.replace(/\s+#.*$/, ""))
      .join("\n");
    try {
      await navigator.clipboard.writeText(text);
      button.textContent = "Copied";
    } catch (e) {
      button.textContent = "Select to copy";
    }
    setTimeout(() => (button.textContent = "Copy"), 1600);
  });
}

/* Syntax highlighting, enough for the snippets on this page. */
const escape = (s) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
const wrap = (cls, s) => `<span class="tok-${cls}">${escape(s)}</span>`;

const LOT_KEYWORDS = new Set(
  ("fn type var if elif else for in while return match case test assert fail and or not is " +
    "None True False inout sink import from as trait impl break continue pass where async await").split(" "),
);
const LOT_TYPES = new Set("int f64 str bool".split(" "));
const LOT_TOKEN =
  /("""[\s\S]*?"""|f?"(?:\\.|[^"\\])*"|#[^\n]*|\b\d+(?:\.\d+)?\b|[A-Za-z_]\w*|->|\?\?|[!?&])/g;

function highlightLot(src) {
  let out = "";
  let last = 0;
  let afterFn = false;
  for (const m of src.matchAll(LOT_TOKEN)) {
    const tok = m[0];
    out += escape(src.slice(last, m.index));
    last = m.index + tok.length;
    if (tok.startsWith('"') || tok.startsWith('f"')) out += wrap("str", tok);
    else if (tok.startsWith("#")) out += wrap("com", tok);
    else if (/^\d/.test(tok)) out += wrap("num", tok);
    else if (LOT_KEYWORDS.has(tok)) out += wrap("kw", tok);
    else if (LOT_TYPES.has(tok) || /^[A-Z]/.test(tok)) out += wrap("ty", tok);
    else if (afterFn) out += wrap("fn", tok);
    else if (/^[A-Za-z_]/.test(tok)) out += escape(tok);
    else out += wrap("op", tok);
    afterFn = tok === "fn";
  }
  return out + escape(src.slice(last));
}

function highlightJson(src) {
  return src.replace(
    /("(?:\\.|[^"\\])*")(\s*:)?|\b(true|false|null)\b|-?\b\d+\b|[&<>]/g,
    (m, str, colon, lit) => {
      if (str) return colon ? wrap("key", str) + colon : wrap("str", str);
      if (lit) return wrap("kw", m);
      if (/\d/.test(m)) return wrap("num", m);
      return escape(m);
    },
  );
}

for (const code of document.querySelectorAll("code.lang-lot")) code.innerHTML = highlightLot(code.textContent);
for (const code of document.querySelectorAll("code.lang-json")) code.innerHTML = highlightJson(code.textContent);

/* The nav gains a border once the page scrolls. */
const nav = document.querySelector(".nav");
const onScroll = () => nav.classList.toggle("scrolled", window.scrollY > 8);
window.addEventListener("scroll", onScroll, { passive: true });
onScroll();

/* Fireflies over the hero: amber and teal points drifting like the banner's pond. */
const canvas = document.querySelector(".fireflies");
const ctx = canvas.getContext("2d");
const COLORS = ["255,194,61", "255,214,120", "47,212,180"];
let flies = [];
let running = false;
let frame = 0;

function resize() {
  const ratio = Math.min(window.devicePixelRatio || 1, 2);
  canvas.width = canvas.clientWidth * ratio;
  canvas.height = canvas.clientHeight * ratio;
  ctx.setTransform(ratio, 0, 0, ratio, 0, 0);
  const count = Math.round(Math.min(56, (canvas.clientWidth * canvas.clientHeight) / 22000));
  flies = Array.from({ length: count }, () => ({
    x: Math.random() * canvas.clientWidth,
    y: Math.random() * canvas.clientHeight,
    vx: (Math.random() - 0.5) * 0.25,
    vy: -0.05 - Math.random() * 0.2,
    size: 1 + Math.round(Math.random() * 2),
    phase: Math.random() * Math.PI * 2,
    color: COLORS[Math.floor(Math.random() * COLORS.length)],
  }));
}

function draw() {
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  ctx.clearRect(0, 0, w, h);
  for (const f of flies) {
    const alpha = 0.35 + 0.45 * Math.sin(f.phase + frame * 0.03);
    ctx.fillStyle = `rgba(${f.color},${alpha.toFixed(3)})`;
    ctx.shadowColor = `rgba(${f.color},0.9)`;
    ctx.shadowBlur = 8;
    /* square points, to stay pixel art */
    ctx.fillRect(Math.round(f.x), Math.round(f.y), f.size * 2, f.size * 2);
  }
}

function tick() {
  if (!running) return;
  frame += 1;
  const w = canvas.clientWidth;
  const h = canvas.clientHeight;
  for (const f of flies) {
    f.x += f.vx + Math.sin((frame + f.phase * 50) * 0.01) * 0.15;
    f.y += f.vy;
    if (f.y < -10) f.y = h + 10;
    if (f.x < -10) f.x = w + 10;
    if (f.x > w + 10) f.x = -10;
  }
  draw();
  requestAnimationFrame(tick);
}

resize();
draw();
window.addEventListener("resize", () => {
  resize();
  draw();
});

if (!reducedMotion && "IntersectionObserver" in window) {
  new IntersectionObserver(([entry]) => {
    const was = running;
    running = entry.isIntersecting && !document.hidden;
    if (running && !was) requestAnimationFrame(tick);
  }).observe(canvas);
}
