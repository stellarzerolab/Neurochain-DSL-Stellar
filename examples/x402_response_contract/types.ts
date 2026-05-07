export type X402PaymentState =
  | "payment_required"
  | "finalized"
  | "replay_blocked"
  | "expired";

export type X402DecisionStatus = "approved" | "blocked" | "not_evaluated";

export type X402GuardrailState = "passed" | "blocked" | "not_run";

export type X402GuardrailExitCode = 3 | 4 | 5 | null;

export type X402DecisionReason =
  | "allowlist"
  | "contract_policy"
  | "intent_safety"
  | "payment_replay_blocked"
  | "payment_expired"
  | null;

export type X402GuardrailReason =
  | "allowlist"
  | "contract_policy"
  | "intent_safety"
  | null;

export type X402ResponseError =
  | "payment_required"
  | "payment_replay_blocked"
  | "payment_expired"
  | null;

export interface X402Payment {
  protocol: "x402";
  state: X402PaymentState;
  challenge_id: string;
  amount: string;
  asset: string;
  network: string;
  receiver: string;
  created_at: number | null;
  expires_at: number | null;
  finalized_at: number | null;
}

export interface X402Decision {
  status: X402DecisionStatus;
  approved: boolean;
  blocked: boolean;
  requires_approval: boolean;
  reason: X402DecisionReason;
}

export interface X402Guardrails {
  state: X402GuardrailState;
  exit_code: X402GuardrailExitCode;
  reason: X402GuardrailReason;
}

export interface X402ActionPlan {
  actions: X402Action[];
  warnings: string[];
  [key: string]: unknown;
}

export interface X402Action {
  kind: string;
  [key: string]: unknown;
}

export interface X402IntentPlanResponse {
  ok: boolean;
  audit_id: string;
  payment: X402Payment;
  decision: X402Decision;
  guardrails: X402Guardrails;
  logs: string[];
  blocked?: boolean | null;
  exit_code?: X402GuardrailExitCode;
  error?: X402ResponseError;
  challenge_id?: string;
  amount?: string;
  asset?: string;
  network?: string;
  receiver?: string;
  expires_at?: number;
  payment_header?: string;
  mock_signature?: string;
  plan?: X402ActionPlan;
  [key: string]: unknown;
}

export type X402PaymentRequiredResponse = X402IntentPlanResponse & {
  ok: false;
  payment: X402Payment & { state: "payment_required" };
  decision: X402Decision & { status: "not_evaluated"; reason: null };
  guardrails: X402Guardrails & { state: "not_run"; exit_code: null; reason: null };
  error: "payment_required";
  challenge_id: string;
  payment_header: "PAYMENT-SIGNATURE";
};

export type X402FinalizedResponse = X402IntentPlanResponse & {
  payment: X402Payment & { state: "finalized" };
  plan: X402ActionPlan;
};

export type X402ReplayBlockedResponse = X402IntentPlanResponse & {
  ok: false;
  payment: X402Payment & { state: "replay_blocked" };
  decision: X402Decision & {
    status: "blocked";
    reason: "payment_replay_blocked";
  };
  guardrails: X402Guardrails & { state: "not_run"; exit_code: null; reason: null };
  error: "payment_replay_blocked";
};

export type X402ExpiredResponse = X402IntentPlanResponse & {
  ok: false;
  payment: X402Payment & { state: "expired" };
  decision: X402Decision & { status: "blocked"; reason: "payment_expired" };
  guardrails: X402Guardrails & { state: "not_run"; exit_code: null; reason: null };
  error: "payment_expired";
};
