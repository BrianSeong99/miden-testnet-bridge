const storage = {
  inboundWallet: "miden.bridge.lab.inboundWallet",
  outboundWallet: "miden.bridge.lab.outboundWallet",
  claimResult: "miden.bridge.lab.claimResult",
  filter: "miden.bridge.lab.filter",
};

const state = {
  flows: new Map(),
  selectedId: null,
  filter: localStorage.getItem(storage.filter) || "all",
  inboundWallet: JSON.parse(localStorage.getItem(storage.inboundWallet) || "null"),
  outboundWallet: JSON.parse(localStorage.getItem(storage.outboundWallet) || "null"),
  claimResult: JSON.parse(localStorage.getItem(storage.claimResult) || "null"),
};

const els = {
  health: document.querySelector("#api-health"),
  profile: document.querySelector("#runtime-profile"),
  demo: document.querySelector("#demo-state"),
  tokenCount: document.querySelector("#token-count"),
  error: document.querySelector("#error-panel"),
  flowList: document.querySelector("#flow-list"),
  artifacts: document.querySelector("#artifact-json"),
  statusCounts: document.querySelector("#status-counts"),
  startInbound: document.querySelector("#start-inbound"),
  claimInbound: document.querySelector("#claim-inbound"),
  fundOutbound: document.querySelector("#fund-outbound"),
  submitOutbound: document.querySelector("#submit-outbound"),
  refreshFlows: document.querySelector("#refresh-flows"),
  copyArtifacts: document.querySelector("#copy-artifacts"),
  inboundAmount: document.querySelector("#inbound-amount"),
  outboundAmount: document.querySelector("#outbound-amount"),
  filters: [...document.querySelectorAll("[data-filter]")],
};

const statuses = [
  "KNOWN_DEPOSIT_TX",
  "PENDING_DEPOSIT",
  "INCOMPLETE_DEPOSIT",
  "PROCESSING",
  "SUCCESS",
  "REFUNDED",
  "FAILED",
];

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function showError(error) {
  els.error.hidden = false;
  els.error.textContent = error instanceof Error ? error.message : String(error);
}

function clearError() {
  els.error.hidden = true;
  els.error.textContent = "";
}

async function api(path, options = {}) {
  const response = await fetch(path, {
    headers: { "content-type": "application/json" },
    ...options,
  });
  const text = await response.text();
  const body = text ? JSON.parse(text) : null;
  if (!response.ok) {
    throw new Error(body?.message || `${path} returned ${response.status}`);
  }
  return body;
}

async function boot() {
  setActiveFilter();
  await Promise.allSettled([refreshRuntime(), refreshFlows()]);
  syncButtons();
  setInterval(refreshActiveFlows, 2500);
}

async function refreshRuntime() {
  try {
    const health = await fetch("/healthz");
    els.health.textContent = health.ok ? "ok" : `http ${health.status}`;
  } catch {
    els.health.textContent = "offline";
  }

  try {
    const info = await api("/demo/info");
    els.profile.textContent = info.runtimeProfile;
    els.demo.textContent = info.demoEnabled ? "enabled" : "disabled";
  } catch (error) {
    els.demo.textContent = "unavailable";
    showError(error);
  }

  try {
    const tokens = await api("/v0/tokens");
    els.tokenCount.textContent = String(tokens.length);
  } catch {
    els.tokenCount.textContent = "error";
  }
}

function syncButtons() {
  els.claimInbound.disabled = !state.inboundWallet;
  els.submitOutbound.disabled = !state.outboundWallet;
}

function setActiveFilter() {
  els.filters.forEach((button) => {
    const active = button.dataset.filter === state.filter;
    button.classList.toggle("active", active);
    button.setAttribute("aria-pressed", String(active));
  });
}

async function withBusy(button, task) {
  clearError();
  const oldText = button.textContent;
  button.disabled = true;
  button.textContent = "Working";
  try {
    await task();
  } catch (error) {
    showError(error);
  } finally {
    button.textContent = oldText;
    button.disabled = false;
    syncButtons();
  }
}

async function startInbound() {
  await withBusy(els.startInbound, async () => {
    const response = await api("/demo/flows/inbound/start", {
      method: "POST",
      body: JSON.stringify({ amount: els.inboundAmount.value, asset: "eth" }),
    });
    state.inboundWallet = response.wallet;
    localStorage.setItem(storage.inboundWallet, JSON.stringify(response.wallet));
    upsertFlow(response.flow);
    selectFlow(response.flow.correlationId);
  });
}

async function claimInbound() {
  await withBusy(els.claimInbound, async () => {
    const response = await api("/demo/flows/inbound/claim", {
      method: "POST",
      body: JSON.stringify({ accountId: state.inboundWallet.accountId }),
    });
    state.claimResult = response;
    localStorage.setItem(storage.claimResult, JSON.stringify(response));
    render();
  });
}

async function fundOutbound() {
  await withBusy(els.fundOutbound, async () => {
    const response = await api("/demo/flows/outbound/fund", {
      method: "POST",
      body: JSON.stringify({ amount: els.outboundAmount.value, asset: "eth" }),
    });
    state.outboundWallet = response.wallet;
    localStorage.setItem(storage.outboundWallet, JSON.stringify(response.wallet));
    upsertFlow(response.flow);
    selectFlow(response.flow.correlationId);
  });
}

async function submitOutbound() {
  await withBusy(els.submitOutbound, async () => {
    const response = await api("/demo/flows/outbound/submit", {
      method: "POST",
      body: JSON.stringify({
        senderAccountId: state.outboundWallet.accountId,
        amount: els.outboundAmount.value,
        asset: "eth",
      }),
    });
    upsertFlow(response.flow);
    selectFlow(response.flow.correlationId);
  });
}

async function refreshFlows() {
  try {
    await refreshRuntime();
    const summaries = await api("/demo/flows");
    await Promise.all(summaries.slice(0, 16).map((flow) => loadFlow(flow.correlationId)));
    render();
  } catch (error) {
    showError(error);
  }
}

async function refreshActiveFlows() {
  const active = [...state.flows.values()].filter((flow) =>
    ["PENDING_DEPOSIT", "KNOWN_DEPOSIT_TX", "PROCESSING"].includes(flow.status),
  );
  await Promise.all(active.map((flow) => loadFlow(flow.correlationId).catch(showError)));
  render();
}

async function loadFlow(id) {
  const flow = await api(`/demo/flows/${id}`);
  upsertFlow(flow);
}

function upsertFlow(flow) {
  state.flows.set(flow.correlationId, flow);
}

function selectFlow(id) {
  state.selectedId = id;
  render();
}

function setFilter(filter) {
  state.filter = filter;
  localStorage.setItem(storage.filter, filter);
  setActiveFilter();
  render();
}

function render() {
  syncButtons();
  const flows = [...state.flows.values()].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
  renderStatusCounts(flows);
  const visible = flows.filter(flowMatchesFilter);

  if (visible.length === 0) {
    els.flowList.innerHTML = `<article class="flow-card"><p class="flow-title">No matching flows</p><p class="flow-meta">Start a demo flow or adjust the status filter.</p></article>`;
  } else {
    els.flowList.innerHTML = visible.map(renderFlow).join("");
    els.flowList.querySelectorAll("[data-flow-id]").forEach((node) => {
      node.addEventListener("click", () => selectFlow(node.dataset.flowId));
    });
  }

  const selected = state.flows.get(state.selectedId) || visible[0] || flows[0];
  if (selected && !state.selectedId) {
    state.selectedId = selected.correlationId;
  }
  els.artifacts.textContent = JSON.stringify(selected ? selectedArtifact(selected) : {}, null, 2);
}

function flowMatchesFilter(flow) {
  if (state.filter === "active") {
    return ["KNOWN_DEPOSIT_TX", "PENDING_DEPOSIT", "PROCESSING"].includes(flow.status);
  }
  if (state.filter === "terminal") {
    return ["INCOMPLETE_DEPOSIT", "SUCCESS", "REFUNDED", "FAILED"].includes(flow.status);
  }
  return true;
}

function renderStatusCounts(flows) {
  const counts = Object.fromEntries(statuses.map((status) => [status, 0]));
  flows.forEach((flow) => {
    counts[flow.status] = (counts[flow.status] || 0) + 1;
  });
  els.statusCounts.innerHTML = statuses
    .map(
      (status) => `
        <div class="status-chip">
          <span>${escapeHtml(status.replaceAll("_", " "))}</span>
          <strong>${counts[status] || 0}</strong>
        </div>
      `,
    )
    .join("");
}

function renderFlow(flow) {
  const selected = state.selectedId === flow.correlationId ? " active" : "";
  const title = flow.direction === "evm-to-miden" ? "Sepolia to Miden" : "Miden to Sepolia";
  return `
    <article class="flow-card${selected}" data-flow-id="${escapeHtml(flow.correlationId)}" tabindex="0">
      <div class="flow-head">
        <div>
          <p class="flow-title">${escapeHtml(title)}</p>
          <div class="flow-meta">
            <span>${escapeHtml(flow.correlationId)}</span>
            <span>${escapeHtml(flow.quoteResponse.quoteRequest.originAsset)} -> ${escapeHtml(flow.quoteResponse.quoteRequest.destinationAsset)}</span>
            <span>${escapeHtml(flow.updatedAt)}</span>
          </div>
        </div>
        <span class="status-pill">${escapeHtml(flow.status)}</span>
      </div>
      ${renderRail(flow)}
      <div class="event-list">
        ${flow.lifecycle.map(renderEvent).join("") || `<div class="event-row"><span>waiting</span><strong>No events yet</strong><span></span></div>`}
      </div>
    </article>
  `;
}

function renderRail(flow) {
  const names =
    flow.direction === "evm-to-miden"
      ? ["Quote", "Sepolia deposit", "Bridge API", "Miden P2ID", "Wallet claim"]
      : ["Funding quote", "Miden wallet", "BridgeOutV1", "Bridge consume", "Sepolia release"];
  const index = statusIndex(flow.status);
  return `<div class="rail">${names
    .map((name, i) => {
      const cls = i < index ? "done" : i === index ? "active" : "";
      return `<div class="rail-step ${cls}"><span>step ${i + 1}</span><strong>${escapeHtml(name)}</strong></div>`;
    })
    .join("")}</div>`;
}

function statusIndex(status) {
  switch (status) {
    case "PENDING_DEPOSIT":
      return 1;
    case "KNOWN_DEPOSIT_TX":
      return 2;
    case "PROCESSING":
      return 3;
    case "SUCCESS":
    case "REFUNDED":
    case "FAILED":
    case "INCOMPLETE_DEPOSIT":
      return 4;
    default:
      return 0;
  }
}

function renderEvent(event) {
  return `
    <div class="event-row">
      <span>${escapeHtml(event.createdAt)}</span>
      <strong>${escapeHtml(event.eventKind)}</strong>
      <span>${escapeHtml(event.toStatus)}</span>
    </div>
  `;
}

function selectedArtifact(flow) {
  return {
    correlationId: flow.correlationId,
    direction: flow.direction,
    status: flow.status,
    depositAddress: flow.quoteResponse.quote.depositAddress,
    depositMemo: flow.quoteResponse.quote.depositMemo,
    artifacts: flow.artifacts,
    inboundWallet: state.inboundWallet,
    outboundWallet: state.outboundWallet,
    claimResult: state.claimResult,
    quoteRequest: flow.quoteResponse.quoteRequest,
  };
}

async function copyArtifacts() {
  await navigator.clipboard.writeText(els.artifacts.textContent);
  const oldText = els.copyArtifacts.textContent;
  els.copyArtifacts.textContent = "Copied";
  setTimeout(() => {
    els.copyArtifacts.textContent = oldText;
  }, 1200);
}

els.startInbound.addEventListener("click", startInbound);
els.claimInbound.addEventListener("click", claimInbound);
els.fundOutbound.addEventListener("click", fundOutbound);
els.submitOutbound.addEventListener("click", submitOutbound);
els.refreshFlows.addEventListener("click", refreshFlows);
els.copyArtifacts.addEventListener("click", () => copyArtifacts().catch(showError));
els.filters.forEach((button) => {
  button.addEventListener("click", () => setFilter(button.dataset.filter));
});

boot();
