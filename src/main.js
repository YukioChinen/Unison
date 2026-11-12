const { invoke } = window.__TAURI__.core;

const $ = (id) => document.getElementById(id);
const hex = (n, w = 4) => n.toString(16).toUpperCase().padStart(w, "0");

const NAMES = { 0x3554: "Redragon M916 PRO 4K", 0x373b: "ATK N9 Ultra" };

async function refresh() {
  $("status").textContent = "Lendo…";
  try {
    const list = await invoke("list_hid_devices", { onlyKnown: $("onlyKnown").checked });
    const tbody = $("devices").querySelector("tbody");
    tbody.innerHTML = "";
    for (const d of list) {
      const tr = document.createElement("tr");
      const name = NAMES[d.vendor_id] || d.product || "Desconhecido";
      tr.innerHTML = `
        <td>${name}<br><small class="hint">${d.product}</small></td>
        <td class="mono">${hex(d.vendor_id)}:${hex(d.product_id)}</td>
        <td>${d.interface}</td>
        <td class="mono">0x${hex(d.usage_page)}</td>
        <td class="mono">0x${hex(d.usage)}</td>
        <td><button class="ghost">Escutar</button></td>`;
      tr.querySelector("button").onclick = () => sniff(d, name);
      tbody.appendChild(tr);
    }
    $("status").textContent = `${list.length} interface(s)`;
  } catch (e) {
    $("status").textContent = "Erro: " + e;
  }
}

async function sniff(d, name) {
  $("sniffer").hidden = false;
  $("sniffTitle").textContent =
    `${name} · interface ${d.interface} · page 0x${hex(d.usage_page)}`;
  $("reports").textContent = "Escutando por 8 s…";
  try {
    const reports = await invoke("sniff_reports", { path: d.path, millis: 8000 });
    $("reports").textContent = reports.length
      ? reports.join("\n")
      : "Nenhum relatório recebido. Tente outra interface ou mexa no dispositivo.";
  } catch (e) {
    $("reports").textContent = "Erro: " + e;
  }
}

// ---- Bateria do headset ----
function renderHeadset(s) {
  const card = $("hsCard");
  const pct = s.percent;
  card.classList.toggle("charging", !!s.charging);
  card.classList.toggle("low", !s.charging && pct != null && pct <= 20);
  $("hsPct").textContent = pct == null ? "—" : Math.round(pct);
  $("hsUnit").textContent = pct == null ? "" : "%";
  $("hsBar").style.width = (pct == null ? 0 : Math.max(0, Math.min(100, pct))) + "%";

  let meta;
  if (s.status === "not_found") meta = "Dongle do headset não encontrado.";
  else if (s.status === "port_busy") meta = "Porta serial ocupada. Feche o ATK HUB (e o serviço dele).";
  else if (s.status === "no_reply") meta = "Sem resposta do headset.";
  else if (!s.headset_on) meta = "Headset desligado ou fora de alcance.";
  else if (s.charging && s.estimated) meta = "⚡ Carregando · valor estimado (o headset informa 100% na carga)";
  else if (s.charging) meta = "⚡ Carregando · nível anterior desconhecido";
  else meta = "Descarregando";
  $("hsMeta").textContent = meta;
}

// Registra o listener ANTES de ler o estado e ainda consulta a cada 2 s: leituras
// estáveis (ex.: headset em 70%) não geram eventos novos depois do primeiro.
async function bindBattery(command, event, render) {
  try {
    await window.__TAURI__.event.listen(event, (e) => render(e.payload));
  } catch (_) {}
  const tick = () => invoke(command).then(render).catch(() => {});
  tick();
  setInterval(tick, 2000);
}
bindBattery("get_headset_battery", "headset-battery", renderHeadset);

// ---- Bateria do mouse ----
function renderMouse(s) {
  const card = $("msCard");
  const pct = s.percent;
  card.classList.toggle("charging", !!s.charging);
  card.classList.toggle("low", !s.charging && pct != null && pct <= 20);
  $("msPct").textContent = pct == null ? "—" : pct;
  $("msUnit").textContent = pct == null ? "" : "%";
  $("msBar").style.width = (pct == null ? 0 : Math.max(0, Math.min(100, pct))) + "%";

  let meta;
  if (s.status === "not_found") meta = "Dongle do mouse não encontrado.";
  else if (s.status === "open_failed") meta = "Não foi possível abrir o dispositivo.";
  else if (s.status === "no_reply") meta = "Sem resposta do mouse.";
  else if (!s.available) meta = "Mouse desligado ou fora de alcance.";
  else {
    const v = s.voltage_mv ? ` · ${(s.voltage_mv / 1000).toFixed(2)} V` : "";
    const via = s.wired ? " · via cabo" : " · via dongle 2.4G";
    meta = s.charging
      ? "⚡ Carregando" + via + v + " · pode estar ~5–10% acima do real"
      : "Descarregando" + via + v;
  }
  $("msMeta").textContent = meta;
}

bindBattery("get_mouse_battery", "mouse-battery", renderMouse);

// ---- Configurações do mouse (polling, estágios de DPI, cores) ----
const RATES = [125, 250, 500, 1000, 2000, 4000];
const toHex = ([r, g, b]) => "#" + [r, g, b].map((v) => v.toString(16).padStart(2, "0")).join("");
const fromHex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
let cfg = null;

function cfgStatus(text, isErr = false) {
  $("cfgStatus").textContent = text;
  $("cfgStatus").classList.toggle("err", isErr);
}

async function cfgRun(label, fn) {
  const panel = $("cfgPanel");
  panel.classList.add("busy");
  cfgStatus(label);
  try {
    await fn();
    cfg = await invoke("mouse_get_config");
    renderCfg();
    cfgStatus("Salvo no mouse ✓ (conferido por leitura)");
  } catch (e) {
    cfgStatus(String(e), true);
  } finally {
    panel.classList.remove("busy");
  }
}

async function loadCfg() {
  const panel = $("cfgPanel");
  panel.classList.add("busy");
  cfgStatus("Lendo do mouse…");
  try {
    cfg = await invoke("mouse_get_config");
    renderCfg();
    cfgStatus(`Lido via ${cfg.via}`);
  } catch (e) {
    cfgStatus(String(e), true);
  } finally {
    panel.classList.remove("busy");
  }
}

function renderCfg() {
  if (!cfg) return;
  const rate = $("cfgRate");
  rate.innerHTML = RATES.map((r) => `<option value="${r}">${r} Hz</option>`).join("");
  if (!RATES.includes(cfg.report_rate)) rate.insertAdjacentHTML("beforeend", `<option value="${cfg.report_rate}">${cfg.report_rate} Hz</option>`);
  rate.value = cfg.report_rate;
  rate.onchange = () => cfgRun("Gravando polling…", () => invoke("mouse_set_report_rate", { hz: +rate.value }));

  const count = $("cfgCount");
  count.innerHTML = [1, 2, 3, 4, 5, 6, 7, 8].map((n) => `<option value="${n}">${n}</option>`).join("");
  count.value = cfg.stage_count;
  count.onchange = () => cfgRun("Gravando quantidade de estágios…", () => invoke("mouse_set_stage_count", { count: +count.value }));

  const tbody = $("stages").querySelector("tbody");
  tbody.innerHTML = "";
  cfg.stages.slice(0, cfg.stage_count).forEach((s, i) => {
    const tr = document.createElement("tr");
    tr.innerHTML = `
      <td><input type="radio" name="activeStage" ${i === cfg.current_stage ? "checked" : ""}></td>
      <td>${i + 1}</td>
      <td><input type="number" min="50" max="26000" step="50" value="${s.dpi}"></td>
      <td><input type="color" value="${toHex(s.color)}"></td>`;
    const [radio, dpi, color] = tr.querySelectorAll("input");
    radio.onchange = () => cfgRun("Gravando estágio ativo…", () => invoke("mouse_set_current_stage", { index: i }));
    const apply = () => {
      const v = Math.round(+dpi.value / 50) * 50;
      cfgRun(`Gravando estágio ${i + 1}…`, () => invoke("mouse_set_stage", { index: i, dpi: v, color: fromHex(color.value) }));
    };
    dpi.onchange = apply;
    color.onchange = apply;
    tbody.appendChild(tr);
  });
  renderLight();
  $("cfgNote").textContent = "Alterações são gravadas direto na memória do mouse. O mouse precisa estar acordado (mexa nele se der erro).";
}

// ---- Iluminação ----
const SPEEDS = [[1, "Lenta"], [3, "Média"], [5, "Rápida"]];
const DPI_EFFECTS = [[0, "Desligado"], [1, "Fixo"], [2, "Respirando"]];
const DPI_BRIGHT = [[16, "Baixo"], [128, "Médio"], [255, "Alto"]];

const opts = (list, cur) => {
  const has = list.some(([v]) => v === cur);
  return (has ? list : [...list, [cur, String(cur)]]).map(([v, t]) => `<option value="${v}">${t}</option>`).join("");
};

function lightStatus(text, isErr = false) {
  $("lightStatus").textContent = text;
  $("lightStatus").classList.toggle("err", isErr);
}

async function lightRun(label, fn) {
  const panel = $("lightPanel");
  panel.classList.add("busy");
  lightStatus(label);
  try {
    await fn();
    cfg = await invoke("mouse_get_config");
    renderCfg();
    lightStatus("Salvo no mouse ✓ (conferido por leitura)");
  } catch (e) {
    lightStatus(String(e), true);
  } finally {
    panel.classList.remove("busy");
  }
}

function renderLight() {
  const d = cfg.dpi_light;
  $("dpiEffect").innerHTML = opts(DPI_EFFECTS, d.effect);
  $("dpiEffect").value = d.effect;
  $("dpiBright").innerHTML = opts(DPI_BRIGHT, d.brightness);
  $("dpiBright").value = d.brightness;
  $("dpiSpeed").innerHTML = opts(SPEEDS, d.speed);
  $("dpiSpeed").value = d.speed;
  const applyDpi = () => lightRun("Gravando indicador de DPI…", () =>
    invoke("mouse_set_dpi_light", {
      effect: +$("dpiEffect").value, brightness: +$("dpiBright").value, speed: +$("dpiSpeed").value,
    }));
  ["dpiEffect", "dpiBright", "dpiSpeed"].forEach((id) => ($(id).onchange = applyDpi));
}

// ---- Iniciar com o Windows ----
async function syncAutostart() {
  try { $("autostart").checked = await invoke("autostart_get"); } catch (_) {}
}
$("autostart").onchange = async () => {
  const want = $("autostart").checked;
  try {
    $("autostart").checked = await invoke("autostart_set", { enabled: want });
  } catch (e) {
    $("autostart").checked = !want;
    alert("Não foi possível alterar: " + e);
  }
};
syncAutostart();
setInterval(syncAutostart, 3000); // acompanha mudanças feitas pelo menu da bandeja

$("cfgReload").onclick = loadCfg;
loadCfg();

$("refresh").onclick = refresh;
$("onlyKnown").onchange = refresh;
refresh();
