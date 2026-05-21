const { invoke } = window.__TAURI__.core;

const $ = (sel) => document.querySelector(sel);
const $$ = (sel) => Array.from(document.querySelectorAll(sel));

const state = {
  listening: [],
  excluded: [],
  declared: { entries: [], scan_paths: [], next_id: 0 },
  refreshedAt: 0,
  isAdmin: false,
  segments: [],
  segIndex: -1,
  scanCandidates: [],
  refinedCandidates: [],
  userLabelOverrides: {},
  currentScanPath: null,
  currentScanLabel: "",
  selectedCandidates: new Set(),
  llmConfig: { base_url: "http://localhost:1234", model: "local-model", batch_size: 25, timeout_secs: 120 },
  llmDebug: [],
};

function escapeHtml(s) {
  if (s == null) return "";
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function showToast(msg, error = false) {
  const el = $("#toast");
  el.textContent = msg;
  el.classList.toggle("error", error);
  el.classList.add("show");
  clearTimeout(showToast._t);
  showToast._t = setTimeout(() => el.classList.remove("show"), 2400);
}

function formatTs(secs) {
  if (!secs) return "尚未刷新";
  const d = new Date(secs * 1000);
  const pad = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/* ---------------- Tabs ---------------- */

function setupTabs() {
  $$(".tab").forEach((btn) => {
    btn.addEventListener("click", () => {
      $$(".tab").forEach((b) => b.classList.remove("active"));
      btn.classList.add("active");
      const tab = btn.dataset.tab;
      $$(".page").forEach((p) => p.classList.remove("active"));
      $(`#page-${tab}`).classList.add("active");
    });
  });
}

/* ---------------- Refresh ---------------- */

async function doRefresh() {
  const btn = $("#btn-refresh");
  btn.disabled = true;
  btn.textContent = "刷新中...";
  try {
    const snap = await invoke("refresh");
    state.listening = snap.listening;
    state.excluded = snap.excluded;
    state.declared = snap.declared;
    state.refreshedAt = snap.refreshed_at;
    state.isAdmin = snap.is_admin;
    renderAdminBadge();
    renderRefreshed();
    renderListening();
    renderExcluded();
    renderDeclared();
    renderScanPaths();
    showToast(`已刷新 · ${state.listening.length} 个监听端口`);
  } catch (e) {
    showToast(`刷新失败：${e}`, true);
  } finally {
    btn.disabled = false;
    btn.textContent = "刷新";
  }
}

function renderAdminBadge() {
  const el = $("#admin-badge");
  const btn = $("#btn-elevate");
  if (state.isAdmin) {
    el.textContent = "已以管理员身份运行";
    el.classList.add("admin-yes");
    el.classList.remove("admin-no");
    if (btn) btn.hidden = true;
  } else {
    el.textContent = "未以管理员运行（部分进程信息不可见）";
    el.classList.add("admin-no");
    el.classList.remove("admin-yes");
    if (btn) btn.hidden = false;
  }
}

async function doElevate() {
  try {
    await invoke("restart_as_admin");
  } catch (e) {
    showToast(`重启失败：${e}`, true);
  }
}

function renderRefreshed() {
  $("#refreshed-at").textContent = `最近刷新：${formatTs(state.refreshedAt)}`;
}

/* ---------------- Listening ---------------- */

function renderListening() {
  const filter = $("#filter-listening").value.toLowerCase().trim();
  const showTcp = $("#filter-tcp").checked;
  const showUdp = $("#filter-udp").checked;
  const rows = state.listening.filter((e) => {
    if (e.protocol === "TCP" && !showTcp) return false;
    if (e.protocol === "UDP" && !showUdp) return false;
    if (!filter) return true;
    return (
      String(e.port).includes(filter) ||
      (e.process_name || "").toLowerCase().includes(filter) ||
      (e.protocol || "").toLowerCase().includes(filter) ||
      (e.local_addr || "").toLowerCase().includes(filter)
    );
  });
  $("#listening-count").textContent = `${rows.length} 条`;
  $("#listening-tbody").innerHTML = rows
    .map(
      (e) => `
    <tr>
      <td class="num">${e.port}</td>
      <td>${escapeHtml(e.protocol)}</td>
      <td>${escapeHtml(e.local_addr)}</td>
      <td>${escapeHtml(e.process_name)}</td>
      <td class="num">${e.pid}</td>
      <td title="${escapeHtml(e.process_path || "")}">${escapeHtml(e.process_path || "")}</td>
    </tr>`,
    )
    .join("");
}

function setupListeningFilters() {
  ["filter-listening", "filter-tcp", "filter-udp"].forEach((id) => {
    $(`#${id}`).addEventListener("input", renderListening);
    $(`#${id}`).addEventListener("change", renderListening);
  });
}

/* ---------------- Excluded ---------------- */

function renderExcluded() {
  $("#excluded-tbody").innerHTML = state.excluded
    .map(
      (r) => `
    <tr>
      <td>${escapeHtml(r.protocol)}</td>
      <td class="num">${r.start}</td>
      <td class="num">${r.end}</td>
      <td class="num">${r.end - r.start + 1}</td>
    </tr>`,
    )
    .join("");
}

/* ---------------- Recommend ---------------- */

async function recomputeSegments() {
  const rangeStart = parseInt($("#range-start").value, 10) || 1024;
  const rangeEnd = parseInt($("#range-end").value, 10) || 65535;
  const minLen = parseInt($("#min-length").value, 10) || 1;
  try {
    const segs = await invoke("free_segments", {
      rangeStart,
      rangeEnd,
      minLength: minLen,
    });
    state.segments = segs;
    state.segIndex = segs.length > 0 ? 0 : -1;
    renderSegment();
    renderAllSegments();
  } catch (e) {
    showToast(`计算失败：${e}`, true);
  }
}

function renderSegment() {
  const seg = state.segIndex >= 0 ? state.segments[state.segIndex] : null;
  if (!seg) {
    $("#seg-range").textContent = "— – —";
    $("#seg-meta").textContent = state.refreshedAt === 0 ? "请先刷新数据" : "未找到符合条件的空闲段";
    $("#seg-counter").textContent = `共 ${state.segments.length} 段`;
    return;
  }
  $("#seg-range").textContent = `${seg.start} – ${seg.end}`;
  $("#seg-meta").textContent = `长度 ${seg.length} · 第 ${state.segIndex + 1} / ${state.segments.length} 段`;
  $("#seg-counter").textContent = `共 ${state.segments.length} 段`;
}

function renderAllSegments() {
  $("#all-segs-tbody").innerHTML = state.segments
    .map(
      (s, i) => `
    <tr data-i="${i}">
      <td class="num">${s.start}</td>
      <td class="num">${s.end}</td>
      <td class="num">${s.length}</td>
    </tr>`,
    )
    .join("");
  $$("#all-segs-tbody tr").forEach((tr) => {
    tr.addEventListener("click", () => {
      state.segIndex = parseInt(tr.dataset.i, 10);
      renderSegment();
    });
  });
}

function setupRecommend() {
  $("#btn-recompute").addEventListener("click", recomputeSegments);
  $("#btn-prev-seg").addEventListener("click", () => {
    if (state.segments.length === 0) return;
    state.segIndex = (state.segIndex - 1 + state.segments.length) % state.segments.length;
    renderSegment();
  });
  $("#btn-next-seg").addEventListener("click", () => {
    if (state.segments.length === 0) return;
    state.segIndex = (state.segIndex + 1) % state.segments.length;
    renderSegment();
  });
  $("#btn-random-seg").addEventListener("click", () => {
    if (state.segments.length === 0) return;
    state.segIndex = Math.floor(Math.random() * state.segments.length);
    renderSegment();
  });
}

/* ---------------- Query ---------------- */

async function doQuery() {
  const port = parseInt($("#query-input").value, 10);
  if (!port || port < 1 || port > 65535) {
    showToast("请输入 1–65535 的端口号", true);
    return;
  }
  try {
    const status = await invoke("query_port", { port });
    renderQueryResult(status);
  } catch (e) {
    showToast(`查询失败：${e}`, true);
  }
}

const HIT_LABELS = {
  listening: "正在监听",
  excluded: "系统排除段",
  iana: "IANA 公认",
  builtin_tool: "常用工具默认",
  declared: "本地声明",
};

function renderQueryResult(status) {
  const headerStatus = status.free
    ? `<span class="query-status-free">✔ 完全空闲</span>`
    : `<span class="query-status-busy">⚠ 被以下来源占用</span>`;
  let body = "";
  if (status.free) {
    body = `<p class="muted">这个端口在 5 类来源中都没有占用记录，可放心使用。</p>`;
  } else {
    body = status.hits
      .map((h) => {
        const kind = h.kind;
        const label = HIT_LABELS[kind] || kind;
        let main = "";
        let meta = "";
        if (kind === "listening") {
          main = `${escapeHtml(h.process_name)} <span class="muted">(PID ${h.pid})</span> · ${escapeHtml(h.protocol)} ${escapeHtml(h.local_addr)}`;
          meta = escapeHtml(h.process_path || "");
        } else if (kind === "excluded") {
          main = `${escapeHtml(h.protocol)} 排除段 ${h.range_start} – ${h.range_end}`;
          meta = "由 Hyper-V / WSL / WinNAT 等系统组件预留";
        } else if (kind === "iana") {
          main = `${escapeHtml(h.name)}`;
          meta = escapeHtml(h.description);
        } else if (kind === "builtin_tool") {
          main = `${escapeHtml(h.name)}`;
          meta = escapeHtml(h.description);
        } else if (kind === "declared") {
          main = `${escapeHtml(h.label)}`;
          meta = `${escapeHtml(h.source_file)}:${h.line} · ${escapeHtml(h.context)}`;
        }
        return `
          <div class="hit ${kind}">
            <span class="hit-kind">${label}</span>
            <span class="hit-body">${main}</span>
            <div class="hit-meta">${meta}</div>
          </div>`;
      })
      .join("");
  }
  $("#query-result").innerHTML = `
    <h3>端口 ${status.port} · ${headerStatus}</h3>
    ${body}
  `;
}

function setupQuery() {
  $("#btn-query").addEventListener("click", doQuery);
  $("#query-input").addEventListener("keydown", (e) => {
    if (e.key === "Enter") doQuery();
  });
}

/* ---------------- Declared ---------------- */

function renderDeclared() {
  const filter = ($("#filter-declared").value || "").toLowerCase().trim();
  const rows = (state.declared.entries || []).filter((e) => {
    if (!filter) return true;
    return (
      String(e.port).includes(filter) ||
      (e.label || "").toLowerCase().includes(filter) ||
      (e.source_file || "").toLowerCase().includes(filter)
    );
  });
  $("#declared-count").textContent = `${rows.length} 条`;
  $("#declared-tbody").innerHTML = rows
    .map(
      (e) => `
    <tr data-id="${e.id}">
      <td class="num">${e.port}</td>
      <td class="editable" data-field="label">${escapeHtml(e.label)}</td>
      <td title="${escapeHtml(e.source_file)}">${escapeHtml(e.source_file)}</td>
      <td class="num">${e.line}</td>
      <td title="${escapeHtml(e.context)}">${escapeHtml(e.context)}</td>
      <td><button class="danger-link" data-action="delete-declared">删除</button></td>
    </tr>`,
    )
    .join("");
  $$("#declared-tbody [data-action='delete-declared']").forEach((btn) => {
    btn.addEventListener("click", async (ev) => {
      const id = parseInt(ev.target.closest("tr").dataset.id, 10);
      try {
        state.declared = await invoke("delete_declared", { id });
        renderDeclared();
        showToast("已删除");
      } catch (e) {
        showToast(`删除失败：${e}`, true);
      }
    });
  });
  $$("#declared-tbody .editable").forEach((td) => {
    td.addEventListener("dblclick", () => startEditLabel(td));
  });
}

function startEditLabel(td) {
  const id = parseInt(td.closest("tr").dataset.id, 10);
  const orig = td.textContent;
  td.innerHTML = `<input value="${escapeHtml(orig)}" />`;
  const input = td.querySelector("input");
  input.focus();
  input.select();
  const commit = async () => {
    const v = input.value.trim();
    if (!v || v === orig) {
      td.textContent = orig;
      return;
    }
    try {
      state.declared = await invoke("update_declared_label", { id, label: v });
      renderDeclared();
      showToast("已更新");
    } catch (e) {
      showToast(`更新失败：${e}`, true);
      td.textContent = orig;
    }
  };
  input.addEventListener("blur", commit);
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") input.blur();
    if (e.key === "Escape") {
      td.textContent = orig;
    }
  });
}

async function clearDeclared() {
  const n = (state.declared.entries || []).length;
  if (n === 0) {
    showToast("声明库已经是空的");
    return;
  }
  if (!confirm(`确认清空声明库？将删除 ${n} 条端口声明，且不可恢复（扫描路径配置保留）。`)) {
    return;
  }
  try {
    const [store, removed] = await invoke("clear_declared");
    state.declared = store;
    renderDeclared();
    renderScanPaths();
    showToast(`已清空声明库（删除 ${removed} 条）`);
  } catch (e) {
    showToast(`清空失败：${e}`, true);
  }
}

function setupDeclared() {
  $("#filter-declared").addEventListener("input", renderDeclared);
  $("#btn-clear-declared").addEventListener("click", clearDeclared);
}

/* ---------------- Scan paths ---------------- */

function renderScanPaths() {
  const rows = state.declared.scan_paths || [];
  $("#scan-paths-tbody").innerHTML = rows
    .map(
      (p) => `
    <tr data-path="${escapeHtml(p.path)}">
      <td>${escapeHtml(p.label)}</td>
      <td title="${escapeHtml(p.path)}">${escapeHtml(p.path)}</td>
      <td class="muted">${p.last_scanned ? formatTs(p.last_scanned) : "—"}</td>
      <td>
        <button data-action="scan-path" class="primary">扫描</button>
        <button data-action="remove-path" class="danger-link">移除</button>
      </td>
    </tr>`,
    )
    .join("");
  $$("#scan-paths-tbody [data-action='scan-path']").forEach((btn) => {
    btn.addEventListener("click", (ev) => {
      const path = ev.target.closest("tr").dataset.path;
      startScan(path);
    });
  });
  $$("#scan-paths-tbody [data-action='remove-path']").forEach((btn) => {
    btn.addEventListener("click", async (ev) => {
      const path = ev.target.closest("tr").dataset.path;
      try {
        state.declared = await invoke("remove_scan_path", { path });
        renderScanPaths();
        showToast("已移除路径");
      } catch (e) {
        showToast(`移除失败：${e}`, true);
      }
    });
  });
}

async function addScanPath() {
  const path = $("#scan-path-input").value.trim();
  if (!path) {
    showToast("请输入路径", true);
    return;
  }
  const label = $("#scan-label-input").value.trim();
  try {
    state.declared = await invoke("add_scan_path", { path, label });
    $("#scan-path-input").value = "";
    $("#scan-label-input").value = "";
    renderScanPaths();
    showToast("已添加路径");
  } catch (e) {
    showToast(`添加失败：${e}`, true);
  }
}

async function browseFolder() {
  try {
    const p = await invoke("pick_folder");
    if (p) $("#scan-path-input").value = p;
  } catch (e) {
    showToast(`选择失败：${e}`, true);
  }
}

async function startScan(path) {
  state.currentScanPath = path;
  const sp = (state.declared.scan_paths || []).find((p) => p.path === path);
  state.currentScanLabel = sp ? sp.label : "";
  state.scanCandidates = [];
  state.refinedCandidates = [];
  state.userLabelOverrides = {};
  state.selectedCandidates = new Set();
  state.llmDebug = [];
  $("#llm-debug-card").hidden = true;
  $("#candidate-card").hidden = false;
  $("#candidate-summary").textContent = "扫描中...";
  $("#candidate-tbody").innerHTML = "";
  $("#llm-progress").textContent = "";
  try {
    const candidates = await invoke("scan_directory", { path });
    state.scanCandidates = candidates;
    state.refinedCandidates = candidates.map(() => null);
    state.scanCandidates.forEach((_, i) => state.selectedCandidates.add(i));
    renderCandidates();
    showToast(`扫描完成，候选 ${candidates.length} 条`);
  } catch (e) {
    $("#candidate-summary").textContent = `扫描失败：${e}`;
    showToast(`扫描失败：${e}`, true);
  }
}

function verdictBadge(refined) {
  if (!refined) return `<span class="muted">—</span>`;
  if (!refined.is_port) {
    return `<span class="hit-kind" style="background:var(--danger);color:#0c1118" title="${escapeHtml(refined.reason || "")}">✗ 否决</span>`;
  }
  const conf = Math.round((refined.confidence || 0) * 100);
  const bg = conf >= 70 ? "var(--success)" : conf >= 40 ? "var(--warn)" : "var(--muted)";
  return `<span class="hit-kind" style="background:${bg};color:#0c1118" title="${escapeHtml(refined.reason || "")}">✓ ${conf}%${refined.role ? " · " + escapeHtml(refined.role) : ""}</span>`;
}

function renderCandidates() {
  const filter = ($("#filter-candidate").value || "").toLowerCase().trim();
  const hideRejected = $("#filter-hide-rejected").checked;
  const rows = state.scanCandidates
    .map((c, i) => ({ c, i, r: state.refinedCandidates[i] }))
    .filter(({ c, r }) => {
      if (hideRejected && r && !r.is_port) return false;
      if (!filter) return true;
      const sw = r && r.software ? r.software.toLowerCase() : "";
      return (
        String(c.port).includes(filter) ||
        c.file.toLowerCase().includes(filter) ||
        c.context.toLowerCase().includes(filter) ||
        (c.keyword || "").toLowerCase().includes(filter) ||
        sw.includes(filter)
      );
    });
  $("#candidate-summary").textContent = `共 ${state.scanCandidates.length} 条 · 路径 ${state.currentScanPath || ""}`;
  $("#candidate-selected-count").textContent = `已勾选 ${state.selectedCandidates.size} 条`;
  $("#candidate-tbody").innerHTML = rows
    .map(({ c, i, r }) => {
      const labelVal =
        state.userLabelOverrides[i] != null
          ? state.userLabelOverrides[i]
          : (r && r.software) || state.currentScanLabel || "";
      return `
    <tr>
      <td><input type="checkbox" data-i="${i}" ${state.selectedCandidates.has(i) ? "checked" : ""} /></td>
      <td class="num">${c.port}</td>
      <td>${verdictBadge(r)}</td>
      <td><input type="text" class="label-input" data-i="${i}" value="${escapeHtml(labelVal)}" placeholder="(留空用文件夹标签)" /></td>
      <td>${escapeHtml(c.keyword)}</td>
      <td title="${escapeHtml(c.file)}">${escapeHtml(c.file)}</td>
      <td class="num">${c.line}</td>
      <td class="wrap" title="${escapeHtml(c.context)}">${escapeHtml(c.context)}</td>
    </tr>`;
    })
    .join("");
  $$("#candidate-tbody input[type='checkbox']").forEach((cb) => {
    cb.addEventListener("change", (e) => {
      const i = parseInt(e.target.dataset.i, 10);
      if (e.target.checked) state.selectedCandidates.add(i);
      else state.selectedCandidates.delete(i);
      $("#candidate-selected-count").textContent = `已勾选 ${state.selectedCandidates.size} 条`;
    });
  });
  $$("#candidate-tbody input.label-input").forEach((inp) => {
    inp.addEventListener("input", (e) => {
      const i = parseInt(e.target.dataset.i, 10);
      state.userLabelOverrides[i] = e.target.value;
    });
  });
}

async function commitScan() {
  if (state.selectedCandidates.size === 0) {
    showToast("没有勾选任何候选", true);
    return;
  }
  const entries = Array.from(state.selectedCandidates)
    .sort((a, b) => a - b)
    .map((i) => {
      const c = state.scanCandidates[i];
      const r = state.refinedCandidates[i];
      const override = state.userLabelOverrides[i];
      const label = override != null
        ? override
        : (r && r.software) || state.currentScanLabel || "";
      return {
        port: c.port,
        label,
        source_file: c.file,
        line: c.line,
        context: c.context,
      };
    });
  const sp = (state.declared.scan_paths || []).find((p) => p.path === state.currentScanPath);
  const groupLabel = sp ? sp.label : "";
  try {
    state.declared = await invoke("commit_entries", {
      entries,
      groupLabel,
      sourcePath: state.currentScanPath,
    });
    $("#candidate-card").hidden = true;
    state.scanCandidates = [];
    state.refinedCandidates = [];
    state.userLabelOverrides = {};
    state.selectedCandidates = new Set();
    renderDeclared();
    renderScanPaths();
    showToast(`已入库 ${entries.length} 条`);
  } catch (e) {
    showToast(`入库失败：${e}`, true);
  }
}

function currentLlmConfigFromForm() {
  return {
    base_url: $("#llm-base-url").value.trim() || "http://localhost:1234",
    model: $("#llm-model").value.trim() || "local-model",
    batch_size: Math.max(1, parseInt($("#llm-batch").value, 10) || 25),
    timeout_secs: state.llmConfig.timeout_secs || 120,
  };
}

async function persistFormConfig() {
  const cfg = currentLlmConfigFromForm();
  state.llmConfig = await invoke("set_llm_config", { cfg });
  return state.llmConfig;
}

async function doLlmRefine() {
  if (state.scanCandidates.length === 0) {
    showToast("没有候选可精炼", true);
    return;
  }
  const btn = $("#btn-llm-refine");
  btn.disabled = true;
  const orig = btn.textContent;
  btn.textContent = "🤖 调用 LM Studio 中...";
  try {
    const applied = await persistFormConfig();
    $("#llm-progress").textContent = `正在精炼 ${state.scanCandidates.length} 条候选，目标：${applied.base_url} ...`;
    const result = await invoke("llm_refine", { candidates: state.scanCandidates });
    state.llmDebug = result.debug || [];
    renderLlmDebug();
    if (result.candidates && result.candidates.length > 0) {
      state.refinedCandidates = result.candidates;
      result.candidates.forEach((r, i) => {
        if (!r.is_port) state.selectedCandidates.delete(i);
      });
      const accepted = result.candidates.filter((r) => r.is_port).length;
      const total = result.candidates.length;
      if (result.success) {
        $("#llm-progress").textContent =
          `精炼完成：接受 ${accepted} / 否决 ${total - accepted}（目标：${applied.base_url}）`;
        showToast(`LLM 精炼完成`);
      } else {
        $("#llm-progress").textContent =
          `部分失败：${result.error}。已处理 ${total} 条，详情见日志。`;
        showToast(`LLM 精炼部分失败，见日志`, true);
      }
      renderCandidates();
    } else {
      $("#llm-progress").textContent = `精炼失败：${result.error || "未知错误"}（详情见下方日志）`;
      showToast(`LLM 精炼失败，见日志`, true);
    }
  } catch (e) {
    $("#llm-progress").textContent = `调用异常：${e}`;
    showToast(`调用异常：${e}`, true);
  } finally {
    btn.disabled = false;
    btn.textContent = orig;
  }
}

function renderLlmDebug() {
  const card = $("#llm-debug-card");
  const content = $("#llm-debug-content");
  if (!state.llmDebug || state.llmDebug.length === 0) {
    card.hidden = true;
    return;
  }
  card.hidden = false;
  card.open = state.llmDebug.some((d) => d.error);
  content.innerHTML = state.llmDebug
    .map((d, i) => {
      const rows = [];
      rows.push(["批次", `#${d.batch_index} (大小 ${d.batch_size}/${state.llmDebug.length === 1 ? "总" : i + 1})`]);
      rows.push(["目标 URL", d.target_url]);
      rows.push(["response_format", d.response_format_used + (d.fallback_attempted ? " · 走过 text 兜底" : "")]);
      rows.push(["max_tokens", String(d.max_tokens_used || "?")]);
      rows.push(["HTTP 状态", d.http_status != null ? String(d.http_status) : "(未收到响应)"]);
      rows.push(["请求体 (截)", d.request_body]);
      rows.push(["响应体 (截)", d.response_body]);
      if (d.parsed_content) rows.push(["model.content (截)", d.parsed_content]);
      if (d.reasoning_content) rows.push(["model.reasoning_content (截)", d.reasoning_content]);
      if (d.error) rows.push(["错误", d.error]);
      const html = rows
        .map(
          ([k, v]) => `
        <div class="dbg-row ${k === "错误" ? "dbg-error" : k === "HTTP 状态" && d.error == null ? "dbg-ok" : ""}">
          <span class="dbg-key">${escapeHtml(k)}</span>
          <span class="dbg-val">${escapeHtml(v || "(空)")}</span>
        </div>`,
        )
        .join("");
      return `<div class="dbg-batch">${html}</div>`;
    })
    .join("");
}

async function loadLlmConfig() {
  try {
    state.llmConfig = await invoke("get_llm_config");
    $("#llm-base-url").value = state.llmConfig.base_url;
    $("#llm-model").value = state.llmConfig.model;
    $("#llm-batch").value = state.llmConfig.batch_size;
    markLlmSaved();
    ["#llm-base-url", "#llm-model", "#llm-batch"].forEach((sel) => {
      $(sel).addEventListener("input", markLlmDirty);
    });
  } catch (e) {
    console.error(e);
  }
}

async function saveLlmConfig() {
  try {
    await persistFormConfig();
    showToast("已保存 LM Studio 设置");
    markLlmSaved();
  } catch (e) {
    showToast(`保存失败：${e}`, true);
  }
}

async function testLlm() {
  $("#llm-status").textContent = "测试中...";
  try {
    const cfg = await persistFormConfig();
    const txt = await invoke("llm_health", { cfg });
    let detail = "通";
    try {
      const parsed = JSON.parse(txt);
      if (parsed.data && parsed.data.length > 0) {
        detail = `通 · 当前加载：${parsed.data.map((m) => m.id).join(", ")}`;
      }
    } catch (_) {}
    $("#llm-status").textContent = "✔ " + detail + " (已保存)";
    $("#llm-status").style.color = "var(--success)";
    markLlmSaved();
  } catch (e) {
    $("#llm-status").textContent = "✗ " + e;
    $("#llm-status").style.color = "var(--danger)";
  }
}

function markLlmDirty() {
  const el = $("#llm-saved-state");
  if (el) {
    el.textContent = "● 未保存（点测试或精炼会自动保存）";
    el.style.color = "var(--warn)";
  }
}

function markLlmSaved() {
  const el = $("#llm-saved-state");
  if (el) {
    el.textContent = "✔ 已保存";
    el.style.color = "var(--success)";
  }
}

function setupScan() {
  $("#btn-add-path").addEventListener("click", addScanPath);
  $("#btn-browse-folder").addEventListener("click", browseFolder);
  $("#scan-path-input").addEventListener("keydown", (e) => {
    if (e.key === "Enter") addScanPath();
  });
  $("#btn-commit-scan").addEventListener("click", commitScan);
  $("#btn-cancel-scan").addEventListener("click", () => {
    $("#candidate-card").hidden = true;
    state.scanCandidates = [];
    state.selectedCandidates = new Set();
  });
  $("#btn-toggle-all").addEventListener("click", () => {
    const allChecked = state.selectedCandidates.size === state.scanCandidates.length;
    if (allChecked) {
      state.selectedCandidates = new Set();
    } else {
      state.selectedCandidates = new Set(state.scanCandidates.map((_, i) => i));
    }
    renderCandidates();
  });
  $("#filter-candidate").addEventListener("input", renderCandidates);
  $("#filter-hide-rejected").addEventListener("change", renderCandidates);
  $("#btn-llm-refine").addEventListener("click", doLlmRefine);
  $("#btn-llm-save").addEventListener("click", saveLlmConfig);
  $("#btn-llm-test").addEventListener("click", testLlm);
}

/* ---------------- Boot ---------------- */

window.addEventListener("DOMContentLoaded", async () => {
  setupTabs();
  setupListeningFilters();
  setupRecommend();
  setupQuery();
  setupDeclared();
  setupScan();
  $("#btn-refresh").addEventListener("click", doRefresh);
  $("#btn-elevate").addEventListener("click", doElevate);

  try {
    state.declared = await invoke("get_declared");
    state.isAdmin = await invoke("is_admin");
    renderAdminBadge();
    renderDeclared();
    renderScanPaths();
    await loadLlmConfig();
  } catch (e) {
    console.error(e);
  }
  await doRefresh();
});
