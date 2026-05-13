const SCENARIOS = [
  {
    file: "payment_required.json",
    label: "Payment required",
    intent: "Agent requests access to NeuroChain before payment.",
  },
  {
    file: "approved.json",
    label: "Approved",
    intent: "Payment finalized and claim_rewards ActionPlan passes guardrails.",
  },
  {
    file: "blocked_exit_3_allowlist.json",
    label: "Blocked exit 3",
    intent: "Allowlist blocks the paid request.",
  },
  {
    file: "blocked_exit_4_contract_policy.json",
    label: "Blocked exit 4",
    intent: "Contract policy rejects the invoke.",
  },
  {
    file: "blocked_exit_5_intent_safety.json",
    label: "Blocked exit 5",
    intent: "Intent safety or typed slots reject the request.",
  },
  {
    file: "replay_blocked.json",
    label: "Replay blocked",
    intent: "A finalized payment proof is reused.",
  },
  {
    file: "expired.json",
    label: "Expired",
    intent: "The x402 challenge expired before finalization.",
  },
  {
    file: "invalid_payment.json",
    label: "Invalid payment",
    intent: "The payment proof is invalid and planner is not run.",
  },
];

const DEMO_ACCOUNT = "GCAL4PIFKWOIFO6YT4T7TSSES7SJCWV7HN7XAUTNFFSGQK74RFUSAJBX";
const DEMO_CONTRACT = "CDLFA6FCYHI7RN3MMTQJV5TUKEYECQJAUE74HD5ZJM4NXMHCN4OJKCIJ";

const LIVE_PRESETS = {
  approved: {
    label: "Live preset: approved",
    status: "preset approved",
    mode: "normal",
    request: {
      prompt: `Invoke contract rewards function claim_rewards for wallet ${DEMO_ACCOUNT}`,
      threshold: 0,
    },
  },
  exit3: {
    label: "Live preset: exit 3 allowlist",
    status: "preset exit 3",
    mode: "normal",
    request: {
      prompt: `Send 5 XLM to ${DEMO_ACCOUNT}`,
      threshold: 0.2,
      allowlist_assets: "USDC:GISSUER",
      allowlist_enforce: true,
    },
  },
  exit4: {
    label: "Live preset: exit 4 contract policy",
    status: "preset exit 4",
    mode: "normal",
    request: {
      prompt: `Invoke contract ${DEMO_CONTRACT} function emergency_withdraw args={"account":"${DEMO_ACCOUNT}"}`,
      threshold: 0,
      contract_policy_enforce: true,
    },
  },
  exit5: {
    label: "Live preset: exit 5 intent safety",
    status: "preset exit 5",
    mode: "normal",
    request: {
      prompt: "Invoke contract rewards function claim_rewards",
      threshold: 0,
    },
  },
  replay: {
    label: "Live preset: replay blocked",
    status: "preset replay",
    mode: "replay",
    request: {
      prompt: `Invoke contract rewards function claim_rewards for wallet ${DEMO_ACCOUNT}`,
      threshold: 0,
    },
  },
};

const SEVERITY_CLASS = {
  info: "info",
  success: "success",
  warning: "warning",
  error: "error",
};

let activeIndex = 0;
let loaded = [];
let lastLiveChallenge = null;
let lastLiveRequest = null;

const elements = {
  scenarioList: document.getElementById("scenarioList"),
  liveBaseUrl: document.getElementById("liveBaseUrl"),
  liveModel: document.getElementById("liveModel"),
  liveThreshold: document.getElementById("liveThreshold"),
  liveApiKey: document.getElementById("liveApiKey"),
  livePrompt: document.getElementById("livePrompt"),
  liveAllowlistContracts: document.getElementById("liveAllowlistContracts"),
  liveAllowlistAssets: document.getElementById("liveAllowlistAssets"),
  liveContractPolicyEnforce: document.getElementById("liveContractPolicyEnforce"),
  liveAllowlistEnforce: document.getElementById("liveAllowlistEnforce"),
  liveStatus: document.getElementById("liveStatus"),
  liveChallengeButton: document.getElementById("liveChallengeButton"),
  liveFinalizeButton: document.getElementById("liveFinalizeButton"),
  liveRunButton: document.getElementById("liveRunButton"),
  fixtureResetButton: document.getElementById("fixtureResetButton"),
  stateTitle: document.getElementById("stateTitle"),
  stateDescription: document.getElementById("stateDescription"),
  stateBadge: document.getElementById("stateBadge"),
  auditId: document.getElementById("auditId"),
  paymentState: document.getElementById("paymentState"),
  decisionState: document.getElementById("decisionState"),
  guardrailState: document.getElementById("guardrailState"),
  flow: document.getElementById("flow"),
  planPreview: document.getElementById("planPreview"),
  paymentPreview: document.getElementById("paymentPreview"),
  rawPreview: document.getElementById("rawPreview"),
  logs: document.getElementById("logs"),
};

init().catch((error) => {
  elements.stateTitle.textContent = "Unable to load fixtures";
  elements.stateDescription.textContent =
    "Serve this directory over localhost so the viewer can fetch the JSON examples.";
  elements.stateBadge.textContent = "load_error";
  elements.stateBadge.className = "badge error";
  elements.rawPreview.textContent = String(error);
});

async function init() {
  wireLiveControls();

  loaded = await Promise.all(
    SCENARIOS.map(async (scenario) => {
      const response = await fetchJson(scenario.file);
      return {
        ...scenario,
        response,
        ui: toUiModel(response),
      };
    }),
  );

  renderScenarioList();
  renderScenario(0);
}

function wireLiveControls() {
  elements.liveChallengeButton.addEventListener("click", () => {
    requestLiveChallenge().catch((error) => renderLiveError(error));
  });
  elements.liveFinalizeButton.addEventListener("click", () => {
    finalizeLivePayment().catch((error) => renderLiveError(error));
  });
  elements.liveRunButton.addEventListener("click", () => {
    runLiveMockFlow().catch((error) => renderLiveError(error));
  });
  elements.fixtureResetButton.addEventListener("click", () => {
    if (loaded.length) {
      renderScenario(Math.max(activeIndex, 0));
    }
    setLiveStatus("fixture mode", "info");
  });

  for (const button of document.querySelectorAll("[data-live-preset]")) {
    button.addEventListener("click", () => {
      runLivePreset(button.dataset.livePreset).catch((error) => renderLiveError(error));
    });
  }
}

async function fetchJson(path) {
  const response = await fetch(path, { cache: "no-store" });
  if (!response.ok) {
    throw new Error(`Failed to load ${path}: ${response.status}`);
  }

  return response.json();
}

async function requestLiveChallenge() {
  setLiveBusy(true);
  setLiveStatus("requesting challenge", "info");

  try {
    const request = buildLiveRequest();
    const result = await postX402IntentPlan(request);
    lastLiveRequest = request;
    lastLiveChallenge = result.body.payment?.challenge_id
      ? {
          challengeId: result.body.payment.challenge_id,
          request,
        }
      : null;

    elements.liveFinalizeButton.disabled = !lastLiveChallenge;
    renderLiveResponse(result, request.prompt, "Live challenge");
    setLiveStatus(lastLiveChallenge ? "challenge ready" : "no challenge id", lastLiveChallenge ? "success" : "warning");
    return result;
  } finally {
    setLiveBusy(false);
  }
}

async function finalizeLivePayment() {
  if (!lastLiveChallenge) {
    throw new Error("Request a live x402 challenge before finalizing.");
  }

  setLiveBusy(true);
  setLiveStatus("finalizing mock payment", "info");

  try {
    const result = await postX402IntentPlan(
      lastLiveChallenge.request,
      `paid:${lastLiveChallenge.challengeId}`,
    );
    renderLiveResponse(result, lastLiveChallenge.request.prompt, "Live finalized");
    elements.liveFinalizeButton.disabled = true;
    setLiveStatus(`finalized HTTP ${result.status}`, result.body.blocked ? "error" : "success");
    return result;
  } finally {
    setLiveBusy(false);
  }
}

async function runLiveMockFlow() {
  const challenge = await requestLiveChallenge();
  if (!challenge.body.payment?.challenge_id) {
    return challenge;
  }

  return finalizeLivePayment();
}

async function runLivePreset(key) {
  const preset = LIVE_PRESETS[key];
  if (!preset) {
    throw new Error(`Unknown live preset: ${key}`);
  }

  applyLivePreset(preset);
  setLiveStatus(preset.status, "info");

  if (preset.mode === "replay") {
    return runLiveReplayFlow(preset);
  }

  return runLiveMockFlow();
}

function applyLivePreset(preset) {
  const request = {
    model: "intent_stellar",
    threshold: 0,
    allowlist_assets: "",
    allowlist_contracts: "",
    allowlist_enforce: false,
    contract_policy_enforce: false,
    ...preset.request,
  };

  elements.liveModel.value = request.model;
  elements.liveThreshold.value = String(request.threshold);
  elements.livePrompt.value = request.prompt;
  elements.liveAllowlistAssets.value = request.allowlist_assets;
  elements.liveAllowlistContracts.value = request.allowlist_contracts;
  elements.liveAllowlistEnforce.checked = Boolean(request.allowlist_enforce);
  elements.liveContractPolicyEnforce.checked = Boolean(request.contract_policy_enforce);
}

async function runLiveReplayFlow(preset) {
  setLiveBusy(true);
  setLiveStatus("running replay preset", "info");

  try {
    const request = buildLiveRequest();
    const challenge = await postX402IntentPlan(request);
    const challengeId = challenge.body.payment?.challenge_id;
    if (!challengeId) {
      renderLiveResponse(challenge, request.prompt, preset.label);
      setLiveStatus("no challenge id", "warning");
      return challenge;
    }

    const signature = `paid:${challengeId}`;
    await postX402IntentPlan(request, signature);
    const replay = await postX402IntentPlan(request, signature);
    renderLiveResponse(replay, request.prompt, preset.label);
    setLiveStatus(`replay HTTP ${replay.status}`, "error");
    lastLiveChallenge = null;
    elements.liveFinalizeButton.disabled = true;
    return replay;
  } finally {
    setLiveBusy(false);
  }
}

function buildLiveRequest() {
  const prompt = elements.livePrompt.value.trim();
  if (!prompt) {
    throw new Error("Live prompt is required.");
  }

  const threshold = Number(elements.liveThreshold.value);
  if (!Number.isFinite(threshold) || threshold < 0 || threshold > 1) {
    throw new Error("Threshold must be a number between 0 and 1.");
  }

  const request = {
    prompt,
    model: elements.liveModel.value.trim() || "intent_stellar",
    threshold,
    contract_policy_enforce: elements.liveContractPolicyEnforce.checked,
    allowlist_enforce: elements.liveAllowlistEnforce.checked,
  };

  const allowlistContracts = elements.liveAllowlistContracts.value.trim();
  if (allowlistContracts) {
    request.allowlist_contracts = allowlistContracts;
  }

  const allowlistAssets = elements.liveAllowlistAssets.value.trim();
  if (allowlistAssets) {
    request.allowlist_assets = allowlistAssets;
  }

  return request;
}

async function postX402IntentPlan(request, paymentSignature = null) {
  const baseUrl = normalizedBaseUrl();
  const headers = {
    "Content-Type": "application/json",
  };

  const apiKey = elements.liveApiKey.value.trim();
  if (apiKey) {
    headers["x-api-key"] = apiKey;
  }

  if (paymentSignature) {
    headers["PAYMENT-SIGNATURE"] = paymentSignature;
  }

  const response = await fetch(`${baseUrl}/api/x402/stellar/intent-plan`, {
    method: "POST",
    headers,
    body: JSON.stringify(request),
  });

  const raw = await response.text();
  let body;
  try {
    body = JSON.parse(raw);
  } catch (error) {
    throw new Error(`Live API returned non-JSON HTTP ${response.status}: ${raw || error.message}`);
  }

  return {
    status: response.status,
    body,
  };
}

function normalizedBaseUrl() {
  const raw = elements.liveBaseUrl.value.trim();
  if (!raw) {
    throw new Error("Server base URL is required.");
  }

  return raw.replace(/\/+$/, "");
}

function renderLiveResponse(result, intent, label) {
  const response = result.body;
  clearFixtureSelection();

  const scenario = {
    file: `HTTP ${result.status}`,
    label,
    intent: `${intent}\n\nHTTP ${result.status} from local API.`,
    response,
    ui: toUiModel(response),
  };

  renderResponseScenario(scenario);
  elements.stateDescription.textContent = `${scenario.ui.description} Live API returned HTTP ${result.status}.`;
}

function renderLiveError(error) {
  clearFixtureSelection();
  setLiveBusy(false);
  setLiveStatus("live error", "error");

  elements.stateTitle.textContent = "Live request failed";
  elements.stateDescription.textContent =
    "Check that neurochain-server is running, CORS is enabled, and the base URL is correct.";
  elements.stateBadge.textContent = "client_error";
  elements.stateBadge.className = "badge error";
  elements.auditId.textContent = "-";
  elements.paymentState.textContent = "-";
  elements.decisionState.textContent = "-";
  elements.guardrailState.textContent = "-";
  elements.flow.replaceChildren();
  renderPlan({ plan: null });
  renderLogs([String(error.message ?? error)]);
  elements.paymentPreview.textContent = "{}";
  elements.rawPreview.textContent = formatJson({
    error: String(error.message ?? error),
    base_url: elements.liveBaseUrl.value.trim(),
  });
}

function setLiveBusy(isBusy) {
  elements.liveChallengeButton.disabled = isBusy;
  elements.liveRunButton.disabled = isBusy;
  elements.fixtureResetButton.disabled = isBusy;
  elements.liveFinalizeButton.disabled = isBusy || !lastLiveChallenge;
  for (const button of document.querySelectorAll("[data-live-preset]")) {
    button.disabled = isBusy;
  }
}

function setLiveStatus(text, severity = "info") {
  elements.liveStatus.textContent = text;
  elements.liveStatus.className = `badge ${SEVERITY_CLASS[severity] ?? "info"}`;
}

function renderScenarioList() {
  elements.scenarioList.replaceChildren(
    ...loaded.map((scenario, index) => {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "scenario-button";
      button.setAttribute("aria-selected", String(index === activeIndex));
      button.addEventListener("click", () => renderScenario(index));

      const title = document.createElement("span");
      title.className = "scenario-title";
      title.textContent = scenario.label;

      const subtitle = document.createElement("span");
      subtitle.className = "scenario-subtitle";
      subtitle.textContent = scenario.file;

      button.append(title, subtitle);
      return button;
    }),
  );
}

function renderScenario(index) {
  activeIndex = index;
  const scenario = loaded[index];
  selectFixtureButton(index);
  renderResponseScenario(scenario);
}

function selectFixtureButton(index) {
  for (const [buttonIndex, button] of [...elements.scenarioList.children].entries()) {
    button.setAttribute("aria-selected", String(buttonIndex === index));
  }
}

function clearFixtureSelection() {
  for (const button of elements.scenarioList.children) {
    button.setAttribute("aria-selected", "false");
  }
}

function renderResponseScenario(scenario) {
  const { response, ui } = scenario;

  elements.stateTitle.textContent = ui.title;
  elements.stateDescription.textContent = ui.description;
  elements.stateBadge.textContent = ui.state;
  elements.stateBadge.className = `badge ${SEVERITY_CLASS[ui.severity] ?? "info"}`;
  elements.auditId.textContent = ui.auditId;
  elements.paymentState.textContent = response.payment.state;
  elements.decisionState.textContent = response.decision.status;
  elements.guardrailState.textContent = guardrailLabel(response.guardrails);

  elements.flow.replaceChildren(...buildFlowSteps(scenario));
  renderPlan(response);
  renderLogs(response.logs ?? []);
  elements.paymentPreview.textContent = formatJson(response.payment);
  elements.rawPreview.textContent = formatJson(response);
}

function buildFlowSteps(scenario) {
  const response = scenario.response;
  const ui = scenario.ui;
  const planAction = response.plan?.actions?.[0];

  const steps = [
    {
      index: "01",
      title: "Agent Request",
      body: scenario.intent,
      state: "active",
    },
    {
      index: "02",
      title: "x402 Payment",
      body: paymentStepText(response),
      state: paymentStepState(response.payment.state),
    },
    {
      index: "03",
      title: "ActionPlan",
      body: planAction
        ? `${planAction.kind}: ${planAction.function ?? planAction.action ?? "planned"}`
        : "No ActionPlan rendered for this payment state.",
      state: response.plan ? "active" : "warn",
    },
    {
      index: "04",
      title: "Guardrails",
      body: guardrailText(response.guardrails),
      state: guardrailStepState(response.guardrails),
    },
    {
      index: "05",
      title: "Decision",
      body: `${response.decision.status}${ui.reason ? `: ${ui.reason}` : ""}`,
      state: decisionStepState(response.decision),
    },
    {
      index: "06",
      title: "Audit Trail",
      body: `${response.audit_id} with ${response.logs?.length ?? 0} log entries.`,
      state: ui.severity === "success" ? "pass" : ui.severity === "error" ? "fail" : "active",
    },
  ];

  return steps.map((step) => {
    const node = document.createElement("article");
    node.className = `step ${step.state}`;

    const index = document.createElement("small");
    index.textContent = step.index;

    const title = document.createElement("strong");
    title.textContent = step.title;

    const body = document.createElement("p");
    body.textContent = step.body;

    node.append(index, title, body);
    return node;
  });
}

function renderPlan(response) {
  if (!response.plan) {
    elements.planPreview.outerHTML =
      '<div class="empty" id="planPreview">No ActionPlan is available because NeuroChain did not run the planner for this response.</div>';
    elements.planPreview = document.getElementById("planPreview");
    return;
  }

  if (elements.planPreview.tagName !== "PRE") {
    elements.planPreview.outerHTML = '<pre id="planPreview"></pre>';
    elements.planPreview = document.getElementById("planPreview");
  }

  elements.planPreview.textContent = formatJson(response.plan);
}

function renderLogs(logs) {
  elements.logs.replaceChildren(
    ...logs.map((entry) => {
      const item = document.createElement("div");
      item.className = "log-item";
      item.textContent = entry;
      return item;
    }),
  );

  if (!logs.length) {
    const empty = document.createElement("div");
    empty.className = "empty";
    empty.textContent = "No logs in this response.";
    elements.logs.append(empty);
  }
}

function toUiModel(response) {
  const base = {
    state: "unknown",
    severity: "info",
    title: "Unknown",
    description: "The response has not been mapped yet.",
    auditId: response.audit_id,
    reason: response.decision?.reason ?? response.guardrails?.reason ?? null,
  };

  switch (response.payment.state) {
    case "payment_required":
      return {
        ...base,
        state: "payment_required",
        title: "Payment required",
        description: "NeuroChain returned an x402 challenge before evaluating the agent request.",
      };
    case "replay_blocked":
      return {
        ...base,
        state: "replay_blocked",
        severity: "error",
        title: "Replay blocked",
        description: "The payment signature was already used. Create a fresh x402 challenge.",
      };
    case "expired":
      return {
        ...base,
        state: "expired",
        severity: "warning",
        title: "Challenge expired",
        description: "The x402 challenge expired before it could be finalized.",
      };
    case "invalid":
      return {
        ...base,
        state: "invalid_payment",
        severity: "error",
        title: "Invalid payment",
        description: "The payment proof was invalid. Create a fresh x402 challenge.",
      };
    case "finalized":
      return finalizedUiModel(response, base);
    default:
      return base;
  }
}

function finalizedUiModel(response, base) {
  if (response.decision.requires_approval) {
    return {
      ...base,
      state: "requires_approval",
      severity: "warning",
      title: "Requires approval",
      description: "Payment finalized, but NeuroChain requires a human approval boundary.",
    };
  }

  if (response.decision.approved || response.decision.status === "approved") {
    return {
      ...base,
      state: "approved",
      severity: "success",
      title: "Approved",
      description: "Payment finalized and NeuroChain approved the typed ActionPlan.",
    };
  }

  if (response.decision.blocked || response.decision.status === "blocked") {
    return blockedUiModel(response, base);
  }

  return base;
}

function blockedUiModel(response, base) {
  switch (response.guardrails.exit_code) {
    case 3:
      return {
        ...base,
        state: "blocked_allowlist",
        severity: "error",
        title: "Blocked by allowlist",
        description: "Payment finalized, but the requested action is outside the allowlist.",
      };
    case 4:
      return {
        ...base,
        state: "blocked_contract_policy",
        severity: "error",
        title: "Blocked by contract policy",
        description: "Payment finalized, but the contract policy rejected the action.",
      };
    case 5:
      return {
        ...base,
        state: "blocked_intent_safety",
        severity: "error",
        title: "Blocked by intent safety",
        description: "Payment finalized, but intent safety, confidence, or typed slots failed.",
      };
    default:
      return {
        ...base,
        state: "blocked_unknown",
        severity: "error",
        title: "Blocked",
        description: "Payment finalized, but NeuroChain blocked the action.",
      };
  }
}

function paymentStepText(response) {
  if (response.payment.state === "finalized") {
    return `Finalized ${response.payment.amount} ${response.payment.asset} on ${response.payment.network}.`;
  }

  if (response.payment.state === "payment_required") {
    return `Challenge ${response.payment.challenge_id} requires ${response.payment.amount} ${response.payment.asset}.`;
  }

  return `${response.payment.state}: create a fresh x402 challenge.`;
}

function paymentStepState(state) {
  switch (state) {
    case "finalized":
      return "pass";
    case "payment_required":
      return "active";
    case "expired":
      return "warn";
    case "replay_blocked":
    case "invalid":
      return "fail";
    default:
      return "active";
  }
}

function guardrailText(guardrails) {
  if (guardrails.state === "not_run") {
    return "Guardrails were not run for this payment state.";
  }

  if (guardrails.exit_code) {
    return `Exit ${guardrails.exit_code}: ${guardrails.reason}`;
  }

  return "Guardrails passed.";
}

function guardrailLabel(guardrails) {
  if (guardrails.exit_code) {
    return `${guardrails.state} / exit ${guardrails.exit_code}`;
  }

  return guardrails.state;
}

function guardrailStepState(guardrails) {
  if (guardrails.state === "passed") {
    return "pass";
  }

  if (guardrails.state === "blocked") {
    return "fail";
  }

  return "warn";
}

function decisionStepState(decision) {
  if (decision.approved || decision.status === "approved") {
    return "pass";
  }

  if (decision.blocked || decision.status === "blocked") {
    return "fail";
  }

  if (decision.requires_approval) {
    return "warn";
  }

  return "active";
}

function formatJson(value) {
  return JSON.stringify(value, null, 2);
}
