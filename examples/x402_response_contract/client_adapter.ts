import type {
  X402ActionPlan,
  X402GuardrailExitCode,
  X402IntentPlanResponse,
} from "./types";

export type X402UiState =
  | "payment_required"
  | "approved"
  | "requires_approval"
  | "blocked_allowlist"
  | "blocked_contract_policy"
  | "blocked_intent_safety"
  | "blocked_unknown"
  | "replay_blocked"
  | "expired"
  | "unknown";

export type X402UiSeverity = "info" | "success" | "warning" | "error";

export interface X402UiModel {
  state: X402UiState;
  severity: X402UiSeverity;
  title: string;
  description: string;
  auditId: string;
  paymentState: X402IntentPlanResponse["payment"]["state"];
  decisionStatus: X402IntentPlanResponse["decision"]["status"];
  guardrailState: X402IntentPlanResponse["guardrails"]["state"];
  exitCode: X402GuardrailExitCode;
  reason: string | null;
  challengeId?: string;
  canRetryWithPayment: boolean;
  requiresFreshChallenge: boolean;
  canRenderPlan: boolean;
  plan?: X402ActionPlan;
  logs: string[];
}

export function toX402UiModel(response: X402IntentPlanResponse): X402UiModel {
  const base = baseUiModel(response);

  switch (response.payment.state) {
    case "payment_required":
      return {
        ...base,
        state: "payment_required",
        severity: "info",
        title: "Payment required",
        description: "NeuroChain returned an x402 challenge before evaluating the agent request.",
        canRetryWithPayment: true,
      };

    case "replay_blocked":
      return {
        ...base,
        state: "replay_blocked",
        severity: "error",
        title: "Replay blocked",
        description: "The payment signature was already used. Create a fresh x402 challenge.",
        requiresFreshChallenge: true,
      };

    case "expired":
      return {
        ...base,
        state: "expired",
        severity: "warning",
        title: "Challenge expired",
        description: "The x402 challenge expired before it could be finalized.",
        requiresFreshChallenge: true,
      };

    case "finalized":
      return finalizedUiModel(response, base);
  }
}

function finalizedUiModel(
  response: X402IntentPlanResponse,
  base: X402UiModel,
): X402UiModel {
  if (response.decision.requires_approval) {
    return {
      ...base,
      state: "requires_approval",
      severity: "warning",
      title: "Requires approval",
      description: "Payment finalized, but NeuroChain requires a human approval boundary.",
      canRenderPlan: Boolean(response.plan),
      plan: response.plan,
    };
  }

  if (response.decision.approved || response.decision.status === "approved") {
    return {
      ...base,
      state: "approved",
      severity: "success",
      title: "Approved",
      description: "Payment finalized and NeuroChain approved the typed ActionPlan.",
      canRenderPlan: Boolean(response.plan),
      plan: response.plan,
    };
  }

  if (response.decision.blocked || response.decision.status === "blocked") {
    return blockedUiModel(response, base);
  }

  return {
    ...base,
    state: "unknown",
    severity: "warning",
    title: "Unknown decision",
    description: "The response is finalized, but the decision state is not recognized.",
    canRenderPlan: Boolean(response.plan),
    plan: response.plan,
  };
}

function blockedUiModel(
  response: X402IntentPlanResponse,
  base: X402UiModel,
): X402UiModel {
  switch (response.guardrails.exit_code) {
    case 3:
      return {
        ...base,
        state: "blocked_allowlist",
        severity: "error",
        title: "Blocked by allowlist",
        description: "Payment finalized, but the requested action is outside the allowlist.",
        canRenderPlan: Boolean(response.plan),
        plan: response.plan,
      };

    case 4:
      return {
        ...base,
        state: "blocked_contract_policy",
        severity: "error",
        title: "Blocked by contract policy",
        description: "Payment finalized, but the contract policy rejected the action.",
        canRenderPlan: Boolean(response.plan),
        plan: response.plan,
      };

    case 5:
      return {
        ...base,
        state: "blocked_intent_safety",
        severity: "error",
        title: "Blocked by intent safety",
        description: "Payment finalized, but intent safety, confidence, or typed slots failed.",
        canRenderPlan: Boolean(response.plan),
        plan: response.plan,
      };

    default:
      return {
        ...base,
        state: "blocked_unknown",
        severity: "error",
        title: "Blocked",
        description: "Payment finalized, but NeuroChain blocked the action.",
        canRenderPlan: Boolean(response.plan),
        plan: response.plan,
      };
  }
}

function baseUiModel(response: X402IntentPlanResponse): X402UiModel {
  return {
    state: "unknown",
    severity: "info",
    title: "Unknown",
    description: "The response has not been mapped yet.",
    auditId: response.audit_id,
    paymentState: response.payment.state,
    decisionStatus: response.decision.status,
    guardrailState: response.guardrails.state,
    exitCode: response.guardrails.exit_code,
    reason: response.decision.reason ?? response.guardrails.reason,
    challengeId: response.challenge_id ?? response.payment.challenge_id,
    canRetryWithPayment: false,
    requiresFreshChallenge: false,
    canRenderPlan: false,
    logs: response.logs,
  };
}
