const state = {
  flows: new Map(),
  selectedId: null,
  inboundWallet: JSON.parse(localStorage.getItem("lab.inboundWallet") || "null"),
  outboundWallet: JSON.parse(localStorage.getItem("lab.outboundWallet") || "null"),
  claimResult: JSON.parse(localStorage.getItem("lab.claimResult") || "null"),
};

const els = {
  health: document.querySelector("#api-health"),
  profile: document.querySelector("#runtime-profile"),
  demo: document.querySelector("#demo-state"),
  error: document.querySelector("#error-panel"),
  flowList: document.querySelector("#flow-list"),
  artifacts: document.querySelector("#artifact-json"),
  startInbound: document.querySelector("#start-inbound"),
  claimInbound: document.querySelector("#claim-inbound"),
  fundOutbound: document.querySelector("#fund-outbound"),
  submitOutbound: document.querySelector("#submit-outbound"),
  refreshFlows: document.querySelector("#refresh-flows"),
  inboundAmount: document.querySelector("#inbound-amount"),
  outboundAmount: document.querySelector("#outbound-amount"),
};

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
  syncButtons();
  await refreshFlows();
  setInterval(refreshActiveFlows, 2500);
}

function syncButtons() {
  els.claimInbound.disabled = !state.inboundWallet;
  els.submitOutbound.disabled = !state.outboundWallet;
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
    localStorage.setItem("lab.inboundWallet", JSON.stringify(response.wallet));
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
    localStorage.setItem("lab.claimResult", JSON.stringify(response));
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
    localStorage.setItem("lab.outboundWallet", JSON.stringify(response.wallet));
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
    const summaries = await api("/demo/flows");
    await Promise.all(summaries.slice(0, 8).map((flow) => loadFlow(flow.correlationId)));
    render();
  } catch (error) {
    showError(error);
  }
}

async function refreshActiveFlows() {
  const active = [...state.flows.values()].filter((flow) =>
    ["PENDING_DEPOSIT", "KNOWN_DEPOSIT_TX", "PROCESSING"].includes(flow.status)
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

function render() {
  syncButtons();
  const flows = [...state.flows.values()].sort((a, b) => b.updatedAt.localeCompare(a.updatedAt));
  if (flows.length === 0) {
    els.flowList.innerHTML = `<article class="flow-card"><p class="flow-title">No flows yet</p><p class="flow-id">Start an inbound or outbound demo flow.</p></article>`;
  } else {
    els.flowList.innerHTML = flows.map(renderFlow).join("");
    els.flowList.querySelectorAll("[data-flow-id]").forEach((node) => {
      node.addEventListener("click", () => selectFlow(node.dataset.flowId));
    });
  }
  const selected = state.flows.get(state.selectedId) || flows[0];
  els.artifacts.textContent = JSON.stringify(selected ? selectedArtifact(selected) : {}, null, 2);
}

function renderFlow(flow) {
  const selected = state.selectedId === flow.correlationId ? " active" : "";
  return `
    <article class="flow-card${selected}" data-flow-id="${flow.correlationId}">
      <div class="flow-head">
        <div>
          <p class="flow-title">${flow.direction === "evm-to-miden" ? "Anvil to Miden" : "Miden to Anvil"}</p>
          <p class="flow-id">${flow.correlationId}</p>
        </div>
        <span class="status-pill">${flow.status}</span>
      </div>
      ${renderRail(flow)}
      <div class="event-list">
        ${flow.lifecycle.map(renderEvent).join("") || `<div class="event-row"><span>waiting</span><strong>No events yet</strong><span></span></div>`}
      </div>
    </article>
  `;
}

function renderRail(flow) {
  const names = flow.direction === "evm-to-miden"
    ? ["Quote", "Anvil deposit", "Bridge API", "Miden P2ID", "Wallet claim"]
    : ["Funding quote", "Miden wallet", "BridgeOutV1", "Bridge consume", "Anvil release"];
  const index = statusIndex(flow.status);
  return `<div class="rail">${names.map((name, i) => {
    const cls = i < index ? "done" : i === index ? "active" : "";
    return `<div class="rail-step ${cls}"><span>step ${i + 1}</span><strong>${name}</strong></div>`;
  }).join("")}</div>`;
}

function statusIndex(status) {
  switch (status) {
    case "PENDING_DEPOSIT": return 1;
    case "KNOWN_DEPOSIT_TX": return 2;
    case "PROCESSING": return 3;
    case "SUCCESS": return 4;
    case "REFUNDED":
    case "FAILED": return 4;
    default: return 0;
  }
}

function renderEvent(event) {
  return `
    <div class="event-row">
      <span>${event.createdAt}</span>
      <strong>${event.eventKind}</strong>
      <span>${event.toStatus}</span>
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

els.startInbound.addEventListener("click", startInbound);
els.claimInbound.addEventListener("click", claimInbound);
els.fundOutbound.addEventListener("click", fundOutbound);
els.submitOutbound.addEventListener("click", submitOutbound);
els.refreshFlows.addEventListener("click", refreshFlows);

boot();
