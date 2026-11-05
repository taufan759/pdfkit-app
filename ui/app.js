"use strict";

const tauri = window.__TAURI__;
const invoke = tauri ? tauri.core.invoke : async () => { throw new Error("Jalankan lewat aplikasi PDFKit (Tauri)."); };

const PDF = { name: "PDF", extensions: ["pdf"] };
const IMAGES = { name: "Gambar", extensions: ["jpg", "jpeg", "png", "webp", "bmp", "gif", "tif", "tiff"] };

const PAGES_HINT = "Contoh: 1,3,5-8. Kosongkan untuk semua halaman.";

// Each tool maps to one `run_tool` call in the Rust backend; `options` builds the form.
const TOOLS = [
  {
    id: "merge", icon: "⧉", title: "Gabung PDF",
    desc: "Satukan beberapa PDF menjadi satu file, urutannya bisa diatur.",
    accept: PDF, multiple: true, minFiles: 2, reorder: true, options: [],
  },
  {
    id: "split", icon: "✂", title: "Pisah PDF",
    desc: "Pecah satu PDF menjadi beberapa file.",
    accept: PDF, multiple: false, minFiles: 1,
    options: [
      { key: "mode", label: "Cara memisah", type: "seg", value: "ranges", choices: [
        ["ranges", "Per rentang"], ["every", "Setiap N halaman"], ["all", "Setiap halaman"],
      ] },
      { key: "ranges", label: "Rentang halaman", type: "text", value: "", placeholder: "1-3, 4-6, 7", hint: "Tiap rentang jadi satu file.", showIf: { mode: "ranges" } },
      { key: "every", label: "Jumlah halaman per file", type: "number", value: 2, min: 1, showIf: { mode: "every" } },
    ],
  },
  {
    id: "compress", icon: "⇲", title: "Kompres PDF",
    desc: "Perkecil ukuran PDF dengan mengompres ulang gambar dan struktur file.",
    accept: PDF, multiple: true, minFiles: 1,
    options: [
      { key: "level", label: "Tingkat kompresi", type: "seg", value: "medium", choices: [
        ["low", "Ringan (kualitas tinggi)"], ["medium", "Rekomendasi"], ["high", "Ekstrem (paling kecil)"],
      ] },
    ],
  },
  {
    id: "rotate", icon: "⟳", title: "Putar PDF",
    desc: "Putar semua halaman atau halaman tertentu.",
    accept: PDF, multiple: true, minFiles: 1,
    options: [
      { key: "angle", label: "Arah putar", type: "seg", value: "90", choices: [["90", "90° kanan"], ["180", "180°"], ["270", "90° kiri"]] },
      { key: "pages", label: "Halaman", type: "text", value: "", placeholder: "Semua halaman", hint: PAGES_HINT },
    ],
  },
  {
    id: "remove", icon: "🗑", title: "Hapus halaman",
    desc: "Buang halaman yang tidak diperlukan.",
    accept: PDF, multiple: false, minFiles: 1,
    options: [
      { key: "pages", label: "Halaman yang dihapus", type: "text", value: "", placeholder: "2, 5-7", required: true },
    ],
  },
  {
    id: "extract", icon: "⎘", title: "Ambil halaman",
    desc: "Salin halaman tertentu ke PDF baru.",
    accept: PDF, multiple: false, minFiles: 1,
    options: [
      { key: "pages", label: "Halaman yang diambil", type: "text", value: "", placeholder: "1, 4-6", required: true },
    ],
  },
  {
    id: "reorder", icon: "⇅", title: "Urutkan halaman",
    desc: "Susun ulang urutan halaman PDF.",
    accept: PDF, multiple: false, minFiles: 1,
    options: [
      { key: "order", label: "Urutan baru", type: "text", value: "", placeholder: "3, 1, 2, 4-10", required: true, hint: "Halaman yang tidak disebut akan dibuang." },
    ],
  },
  {
    id: "pdf_to_image", icon: "🖼", title: "PDF ke JPG",
    desc: "Ubah setiap halaman PDF menjadi gambar.",
    accept: PDF, multiple: false, minFiles: 1,
    options: [
      { key: "format", label: "Format", type: "seg", value: "jpg", choices: [["jpg", "JPG"], ["png", "PNG"]] },
      { key: "dpi", label: "Resolusi", type: "seg", value: "150", choices: [["72", "Rendah (72 dpi)"], ["150", "Normal (150 dpi)"], ["300", "Tinggi (300 dpi)"]] },
      { key: "pages", label: "Halaman", type: "text", value: "", placeholder: "Semua halaman", hint: PAGES_HINT },
    ],
  },
  {
    id: "image_to_pdf", icon: "📄", title: "JPG ke PDF",
    desc: "Gabungkan gambar (JPG, PNG, dan lainnya) menjadi satu PDF.",
    accept: IMAGES, multiple: true, minFiles: 1, reorder: true,
    options: [
      { key: "page_size", label: "Ukuran halaman", type: "seg", value: "fit", choices: [["fit", "Sesuai gambar"], ["a4", "A4"], ["letter", "Letter"]] },
      { key: "orientation", label: "Orientasi", type: "seg", value: "auto", choices: [["auto", "Otomatis"], ["portrait", "Potret"], ["landscape", "Lanskap"]], showIf: { page_size: ["a4", "letter"] } },
      { key: "margin", label: "Margin", type: "seg", value: "0", choices: [["0", "Tanpa"], ["18", "Kecil"], ["36", "Besar"]] },
    ],
  },
  {
    id: "watermark", icon: "💧", title: "Watermark",
    desc: "Tambahkan teks watermark di atas halaman PDF.",
    accept: PDF, multiple: true, minFiles: 1,
    options: [
      { key: "text", label: "Teks watermark", type: "text", value: "RAHASIA", required: true },
      { key: "font_size", label: "Ukuran huruf", type: "number", value: 60, min: 6, max: 300 },
      { key: "opacity", label: "Transparansi", type: "range", value: 30, min: 5, max: 100, suffix: "%" },
      { key: "rotation", label: "Kemiringan", type: "seg", value: "45", choices: [["0", "Datar"], ["45", "Miring 45°"]] },
      { key: "color", label: "Warna", type: "select", value: "red", choices: [["red", "Merah"], ["gray", "Abu-abu"], ["black", "Hitam"], ["blue", "Biru"]] },
      { key: "pages", label: "Halaman", type: "text", value: "", placeholder: "Semua halaman", hint: PAGES_HINT },
    ],
  },
  {
    id: "page_numbers", icon: "#", title: "Nomor halaman",
    desc: "Beri nomor halaman pada PDF.",
    accept: PDF, multiple: true, minFiles: 1,
    options: [
      { key: "position", label: "Posisi", type: "select", value: "bottom-center", choices: [
        ["bottom-center", "Bawah tengah"], ["bottom-right", "Bawah kanan"], ["bottom-left", "Bawah kiri"],
        ["top-center", "Atas tengah"], ["top-right", "Atas kanan"], ["top-left", "Atas kiri"],
      ] },
      { key: "format", label: "Format", type: "select", value: "{n}", choices: [["{n}", "1"], ["{n} / {total}", "1 / 10"], ["Halaman {n}", "Halaman 1"], ["Halaman {n} dari {total}", "Halaman 1 dari 10"]] },
      { key: "start", label: "Mulai dari angka", type: "number", value: 1, min: 0 },
      { key: "font_size", label: "Ukuran huruf", type: "number", value: 11, min: 6, max: 72 },
      { key: "pages", label: "Halaman yang diberi nomor", type: "text", value: "", placeholder: "Semua halaman", hint: PAGES_HINT },
    ],
  },
  {
    id: "protect", icon: "🔒", title: "Kunci PDF",
    desc: "Lindungi PDF dengan password (enkripsi AES-256).",
    accept: PDF, multiple: true, minFiles: 1,
    options: [
      { key: "password", label: "Password", type: "password", value: "", required: true },
      { key: "confirm", label: "Ulangi password", type: "password", value: "", required: true },
    ],
  },
  {
    id: "unlock", icon: "🔓", title: "Buka kunci PDF",
    desc: "Hapus password dari PDF yang password-nya kamu ketahui.",
    accept: PDF, multiple: true, minFiles: 1,
    options: [
      { key: "password", label: "Password saat ini", type: "password", value: "", hint: "Kosongkan jika PDF hanya punya batasan izin." },
    ],
  },
  {
    id: "repair", icon: "🛠", title: "Perbaiki PDF",
    desc: "Coba pulihkan PDF yang rusak dengan menulis ulang strukturnya.",
    accept: PDF, multiple: true, minFiles: 1, options: [],
  },
];

const $ = (sel) => document.querySelector(sel);
const state = { tool: null, files: [], values: {} };

function fmtSize(bytes) {
  if (bytes == null) return "";
  const units = ["B", "KB", "MB", "GB"];
  let i = 0;
  let n = bytes;
  while (n >= 1024 && i < units.length - 1) { n /= 1024; i++; }
  return `${n.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

function baseName(p) { return p.split(/[\\/]/).pop(); }

function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else if (k.startsWith("on")) node.addEventListener(k.slice(2), v);
    else if (v !== false && v != null) node.setAttribute(k, v === true ? "" : v);
  }
  for (const c of children.flat()) if (c != null) node.append(c);
  return node;
}

/* ---------- theme ---------- */

function applyTheme(theme) {
  document.documentElement.dataset.theme = theme;
  try { localStorage.setItem("pdfkit-theme", theme); } catch { /* storage unavailable */ }
}

function initTheme() {
  let saved = null;
  try { saved = localStorage.getItem("pdfkit-theme"); } catch { /* storage unavailable */ }
  const prefersDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
  applyTheme(saved || (prefersDark ? "dark" : "light"));
  $("#theme-btn").addEventListener("click", () => {
    applyTheme(document.documentElement.dataset.theme === "dark" ? "light" : "dark");
  });
}

/* ---------- views ---------- */

function renderHome() {
  const grid = $("#tool-grid");
  grid.replaceChildren(...TOOLS.map((t) =>
    el("button", { class: "card", onclick: () => openTool(t.id) },
      el("div", { class: "ico" }, t.icon),
      el("h3", {}, t.title),
      el("p", {}, t.desc))));
}

function showView(id) {
  for (const v of document.querySelectorAll(".view")) v.classList.toggle("hidden", v.id !== id);
  window.scrollTo(0, 0);
}

function openTool(id) {
  const tool = TOOLS.find((t) => t.id === id);
  state.tool = tool;
  state.files = [];
  state.values = Object.fromEntries(tool.options.map((o) => [o.key, o.value]));
  $("#tool-title").textContent = tool.title;
  $("#tool-desc").textContent = tool.desc;
  $("#accept-hint").textContent = `${tool.multiple ? "Bisa beberapa file" : "Satu file"} · ${tool.accept.extensions.map((e) => e.toUpperCase()).join(", ")}`;
  $("#result").classList.add("hidden");
  renderOptions();
  renderFiles();
  showView("tool");
}

function goHome() {
  state.tool = null;
  showView("home");
}

/* ---------- options form ---------- */

function optionVisible(opt) {
  if (!opt.showIf) return true;
  return Object.entries(opt.showIf).every(([k, v]) =>
    Array.isArray(v) ? v.includes(state.values[k]) : state.values[k] === v);
}

function renderOptions() {
  const form = $("#options");
  const fields = state.tool.options.filter(optionVisible).map((opt) => {
    const id = `opt-${opt.key}`;
    const set = (v) => { state.values[opt.key] = v; updateRunButton(); };
    let control;
    switch (opt.type) {
      case "seg":
        control = el("div", { class: "seg", role: "radiogroup" }, opt.choices.map(([val, text]) =>
          el("label", {},
            el("input", {
              type: "radio", name: id, value: val, checked: state.values[opt.key] === val,
              onchange: () => { set(val); renderOptions(); },
            }),
            text)));
        break;
      case "select":
        control = el("select", { id, onchange: (e) => set(e.target.value) },
          opt.choices.map(([val, text]) => el("option", { value: val, selected: state.values[opt.key] === val }, text)));
        break;
      case "range": {
        const out = el("span", {}, `${state.values[opt.key]}${opt.suffix || ""}`);
        control = el("div", {},
          el("input", {
            id, type: "range", min: opt.min, max: opt.max, value: state.values[opt.key],
            oninput: (e) => { set(Number(e.target.value)); out.textContent = `${e.target.value}${opt.suffix || ""}`; },
          }),
          out);
        break;
      }
      case "number":
        control = el("input", {
          id, type: "number", min: opt.min, max: opt.max, value: state.values[opt.key],
          oninput: (e) => set(e.target.value === "" ? "" : Number(e.target.value)),
        });
        break;
      default:
        control = el("input", {
          id, type: opt.type, value: state.values[opt.key], placeholder: opt.placeholder || "",
          oninput: (e) => set(e.target.value),
        });
    }
    return el("div", { class: "field" },
      el("label", { for: id }, opt.label + (opt.required ? " *" : "")),
      control,
      opt.hint ? el("small", {}, opt.hint) : null);
  });
  form.replaceChildren(...fields);
  updateRunButton();
}

/* ---------- files ---------- */

async function addFiles(paths) {
  const tool = state.tool;
  if (!tool) return;
  const exts = tool.accept.extensions;
  const accepted = paths.filter((p) => exts.includes(p.split(".").pop().toLowerCase()));
  if (accepted.length === 0) {
    showError(`File harus berformat ${exts.map((e) => e.toUpperCase()).join(", ")}.`);
    return;
  }
  let infos;
  try {
    infos = await invoke("inspect_files", { paths: accepted });
  } catch {
    infos = accepted.map((path) => ({ path, size: null, pages: null, encrypted: false }));
  }
  if (tool.multiple) {
    const known = new Set(state.files.map((f) => f.path));
    state.files.push(...infos.filter((f) => !known.has(f.path)));
  } else {
    state.files = infos.slice(0, 1);
  }
  $("#result").classList.add("hidden");
  renderFiles();
}

function moveFile(i, delta) {
  const j = i + delta;
  if (j < 0 || j >= state.files.length) return;
  [state.files[i], state.files[j]] = [state.files[j], state.files[i]];
  renderFiles();
}

function renderFiles() {
  const list = $("#file-list");
  const reorder = state.tool.reorder && state.files.length > 1;
  list.replaceChildren(...state.files.map((f, i) => {
    const meta = [fmtSize(f.size), f.pages != null ? `${f.pages} hlm` : null, f.encrypted ? "🔒 terkunci" : null]
      .filter(Boolean).join(" · ");
    return el("li", {},
      el("span", { class: "name", title: f.path }, baseName(f.path)),
      el("span", { class: "meta" }, meta),
      reorder ? el("button", { class: "mini", title: "Naik", onclick: () => moveFile(i, -1) }, "↑") : null,
      reorder ? el("button", { class: "mini", title: "Turun", onclick: () => moveFile(i, 1) }, "↓") : null,
      el("button", { class: "mini", title: "Hapus dari daftar", onclick: () => { state.files.splice(i, 1); renderFiles(); } }, "✕"));
  }));
  updateRunButton();
}

async function pickFiles() {
  const tool = state.tool;
  try {
    const picked = await tauri.dialog.open({ multiple: tool.multiple, filters: [tool.accept] });
    if (!picked) return;
    await addFiles(Array.isArray(picked) ? picked : [picked]);
  } catch (e) {
    showError(String(e));
  }
}

/* ---------- run ---------- */

function validate() {
  const tool = state.tool;
  if (state.files.length < tool.minFiles) {
    return tool.minFiles > 1 ? `Pilih minimal ${tool.minFiles} file.` : "Pilih file dulu.";
  }
  for (const opt of tool.options.filter(optionVisible)) {
    if (opt.required && String(state.values[opt.key] ?? "").trim() === "") return `Isi "${opt.label}".`;
  }
  if (tool.id === "protect" && state.values.password !== state.values.confirm) return "Password tidak sama.";
  return null;
}

function updateRunButton() {
  $("#run-btn").disabled = validate() !== null;
}

async function run() {
  const problem = validate();
  if (problem) { showError(problem); return; }
  const tool = state.tool;
  const options = {};
  for (const opt of tool.options.filter(optionVisible)) options[opt.key] = state.values[opt.key];
  delete options.confirm;

  $("#busy").classList.remove("hidden");
  try {
    const res = await invoke("run_tool", { tool: tool.id, inputs: state.files.map((f) => f.path), options });
    showResult(res);
  } catch (e) {
    showError(String(e));
  } finally {
    $("#busy").classList.add("hidden");
  }
}

function showResult(res) {
  const box = $("#result");
  box.className = "result ok";
  const saved = res.before > 0 && res.after > 0 && state.tool.id === "compress"
    ? el("p", {}, `${fmtSize(res.before)} → ${fmtSize(res.after)} (hemat ${Math.max(0, Math.round((1 - res.after / res.before) * 100))}%)`)
    : null;
  const shown = res.outputs.slice(0, 8);
  box.replaceChildren(
    el("h3", {}, "Selesai ✓"),
    el("p", {}, res.message),
    saved,
    el("ul", {}, shown.map((p) => el("li", { title: p }, baseName(p))),
      res.outputs.length > shown.length ? el("li", {}, `…dan ${res.outputs.length - shown.length} file lagi`) : null),
    el("div", { class: "row" },
      res.outputs.length === 1 ? el("button", { class: "btn primary", onclick: () => invoke("open_path", { path: res.outputs[0] }) }, "Buka file") : null,
      el("button", { class: "btn", onclick: () => invoke("reveal_path", { path: res.outputs[0] }) }, "Tampilkan di folder"),
      el("button", { class: "btn", onclick: () => openTool(state.tool.id) }, "Proses file lain")));
  box.scrollIntoView({ behavior: "smooth", block: "nearest" });
}

function showError(msg) {
  const box = $("#result");
  box.className = "result err";
  box.replaceChildren(el("h3", {}, "Gagal"), el("p", {}, msg));
}

/* ---------- wiring ---------- */

function initDropzone() {
  const dz = $("#dropzone");
  if (tauri && tauri.webview) {
    tauri.webview.getCurrentWebview().onDragDropEvent((event) => {
      const { type, paths } = event.payload;
      if (!state.tool) return;
      if (type === "enter" || type === "over") dz.classList.add("over");
      else dz.classList.remove("over");
      if (type === "drop" && paths && paths.length) addFiles(paths);
    });
  }
}

renderHome();
initTheme();
initDropzone();
$("#home-btn").addEventListener("click", goHome);
$("#back-btn").addEventListener("click", goHome);
$("#pick-btn").addEventListener("click", pickFiles);
$("#run-btn").addEventListener("click", run);
$("#options").addEventListener("submit", (e) => e.preventDefault());
