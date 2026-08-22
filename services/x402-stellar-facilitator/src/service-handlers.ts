import type {
  PaymentPayload,
  PaymentRequirements,
  SettleRequest,
  SettleResponse,
  SupportedResponse,
  VerifyRequest,
  VerifyResponse,
} from "@x402/core/types";
import {
  PaymentPayloadSchema,
  PaymentRequirementsSchema,
} from "@x402/core/schemas";

export const SERVICE_HANDLER_SCHEMA_VERSION = 1 as const;
export const SERVICE_HANDLER_NETWORKS = Object.freeze([
  "stellar:testnet",
  "stellar:pubnet",
] as const);

export const SERVICE_HANDLER_AUTHORITY_BOUNDARY = Object.freeze({
  networkAccessGranted: false,
  credentialUseGranted: false,
  paymentVerificationGranted: false,
  paymentSettlementGranted: false,
  guardrailOverrideGranted: false,
  serviceDispatchGranted: false,
  walletSigningGranted: false,
  transactionSubmitGranted: false,
  actionPlanSubmitGranted: false,
});

type ServiceOperation = "supported" | "verify" | "settle" | "evaluation";
type ServiceStatus = "completed" | "rejected" | "unavailable";

export type ServiceHandlerCode =
  | "supported_ready"
  | "upstream_supported_unavailable"
  | "request_invalid"
  | "unsupported_network"
  | "verify_completed"
  | "verify_rejected"
  | "upstream_verify_response_invalid"
  | "upstream_verify_unavailable"
  | "settle_completed"
  | "settle_rejected"
  | "settlement_unverified"
  | "settlement_duplicate"
  | "settlement_replay"
  | "settlement_state_unavailable"
  | "settlement_outcome_unknown"
  | "settlement_state_response_invalid"
  | "upstream_settle_response_invalid"
  | "upstream_settle_unavailable"
  | "evaluation_completed"
  | "evaluation_request_invalid"
  | "evaluation_response_invalid"
  | "evaluation_request_id_mismatch"
  | "evaluation_unavailable";

export interface ServiceHandlerResult<T> {
  readonly schemaVersion: typeof SERVICE_HANDLER_SCHEMA_VERSION;
  readonly operation: ServiceOperation;
  readonly status: ServiceStatus;
  readonly code: ServiceHandlerCode;
  readonly reason: string;
  readonly response: T | null;
  readonly authorityBoundary: typeof SERVICE_HANDLER_AUTHORITY_BOUNDARY;
}

export interface FacilitatorPort {
  getSupported(): SupportedResponse;
  verify(
    paymentPayload: PaymentPayload,
    paymentRequirements: PaymentRequirements,
  ): Promise<VerifyResponse>;
  settle(
    paymentPayload: PaymentPayload,
    paymentRequirements: PaymentRequirements,
  ): Promise<SettleResponse>;
}

export type SettlementAdmissionCode =
  | "settlement_unverified"
  | "settlement_duplicate"
  | "settlement_replay"
  | "settlement_state_unavailable"
  | "settlement_outcome_unknown";

export type SettlementReservation =
  | Readonly<{ status: "reserved"; reservationId: string }>
  | Readonly<{
      status: "rejected" | "unavailable";
      code: SettlementAdmissionCode;
      reason: string;
    }>;

export type SettlementOutcome =
  | Readonly<{ status: "upstream_response"; response: SettleResponse }>
  | Readonly<{
      status: "upstream_error";
      code:
        | "upstream_settle_response_invalid"
        | "upstream_settle_unavailable";
    }>;

export type SettlementFinalization =
  | Readonly<{ status: "recorded" }>
  | Readonly<{
      status: "unavailable";
      code: "settlement_outcome_unknown" | "settlement_state_unavailable";
      reason: string;
    }>;

export interface SettlementStatePort {
  reserve(request: Readonly<SettleRequest>): Promise<SettlementReservation>;
  finalize(
    reservationId: string,
    outcome: SettlementOutcome,
  ): Promise<SettlementFinalization>;
}

export interface NeuroChainAuthorityGrants {
  readonly payment_verification: false;
  readonly payment_settlement: false;
  readonly guardrail_override: false;
  readonly wallet_signing: false;
  readonly stellar_submission: false;
}

export interface NeuroChainEvaluationRequest {
  readonly schema_version: 1;
  readonly message_type: "evaluation_request";
  readonly request_id: string;
  readonly resource_id: string;
  readonly operation: "plan_and_evaluate";
  readonly network: (typeof SERVICE_HANDLER_NETWORKS)[number];
  readonly intent_text: string;
}

export interface NeuroChainEvaluationResponse {
  readonly schema_version: 1;
  readonly message_type: "evaluation_response";
  readonly request_id: string;
  readonly decision: "approved" | "requires_approval" | "blocked";
  readonly exit_code: 3 | 4 | 5 | null;
  readonly reason_code: string;
  readonly action_plan: Readonly<Record<string, unknown>>;
  readonly action_plan_hash: string;
  readonly authority_grants: NeuroChainAuthorityGrants;
  readonly underlying_action_submit_allowed: false;
}

export interface NeuroChainEvaluationPort {
  evaluate(request: NeuroChainEvaluationRequest): Promise<unknown>;
}

interface Parsed<T> {
  readonly ok: true;
  readonly value: T;
}

interface ParseFailure {
  readonly ok: false;
  readonly reason: string;
}

type ParseResult<T> = Parsed<T> | ParseFailure;

const REQUEST_KEYS = Object.freeze([
  "paymentPayload",
  "paymentRequirements",
  "x402Version",
]);
const EVALUATION_REQUEST_KEYS = Object.freeze([
  "intent_text",
  "message_type",
  "network",
  "operation",
  "request_id",
  "resource_id",
  "schema_version",
]);
const EVALUATION_RESPONSE_KEYS = Object.freeze([
  "action_plan",
  "action_plan_hash",
  "authority_grants",
  "decision",
  "exit_code",
  "message_type",
  "reason_code",
  "request_id",
  "schema_version",
  "underlying_action_submit_allowed",
]);
const ACTION_PLAN_KEYS = Object.freeze([
  "actions",
  "schema_version",
  "source",
  "warnings",
]);
const AUTHORITY_GRANT_KEYS = Object.freeze([
  "guardrail_override",
  "payment_settlement",
  "payment_verification",
  "stellar_submission",
  "wallet_signing",
]);

function result(
  operation: ServiceOperation,
  status: ServiceStatus,
  code: ServiceHandlerCode,
  reason: string,
  response: null,
): ServiceHandlerResult<never>;
function result<T>(
  operation: ServiceOperation,
  status: ServiceStatus,
  code: ServiceHandlerCode,
  reason: string,
  response: T,
): ServiceHandlerResult<T>;
function result<T>(
  operation: ServiceOperation,
  status: ServiceStatus,
  code: ServiceHandlerCode,
  reason: string,
  response: T | null,
): ServiceHandlerResult<T> {
  return Object.freeze({
    schemaVersion: SERVICE_HANDLER_SCHEMA_VERSION,
    operation,
    status,
    code,
    reason,
    response,
    authorityBoundary: SERVICE_HANDLER_AUTHORITY_BOUNDARY,
  });
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function hasExactKeys(
  value: Record<string, unknown>,
  expected: readonly string[],
): boolean {
  const actual = Object.keys(value).sort();
  return (
    actual.length === expected.length &&
    actual.every((key, index) => key === expected[index])
  );
}

function isNonEmptyBoundedString(
  value: unknown,
  maxBytes: number,
): value is string {
  return (
    typeof value === "string" &&
    value.trim().length > 0 &&
    Buffer.byteLength(value, "utf8") <= maxBytes
  );
}

function parseFacilitatorRequest<T extends VerifyRequest | SettleRequest>(
  input: unknown,
): ParseResult<T> {
  if (!isRecord(input) || !hasExactKeys(input, REQUEST_KEYS)) {
    return {
      ok: false,
      reason: "request must use the strict x402 facilitator v2 envelope",
    };
  }
  if (input.x402Version !== 2) {
    return { ok: false, reason: "only x402Version 2 is accepted" };
  }
  const payload = PaymentPayloadSchema.safeParse(input.paymentPayload);
  const requirements = PaymentRequirementsSchema.safeParse(
    input.paymentRequirements,
  );
  if (!payload.success || !requirements.success) {
    return {
      ok: false,
      reason: "payment payload or requirements failed the upstream core schema",
    };
  }
  if (payload.data.x402Version !== input.x402Version) {
    return {
      ok: false,
      reason: "request and payment payload x402 versions do not match",
    };
  }
  return {
    ok: true,
    value: {
      x402Version: input.x402Version,
      paymentPayload: payload.data,
      paymentRequirements: requirements.data,
    } as T,
  };
}

function isSupportedNetwork(
  network: string,
): network is (typeof SERVICE_HANDLER_NETWORKS)[number] {
  return SERVICE_HANDLER_NETWORKS.some((candidate) => candidate === network);
}

function hasNonEmptyReason(value: unknown): value is string {
  return typeof value === "string" && value.trim().length > 0;
}

function isVerifyResponse(value: unknown): value is VerifyResponse {
  if (!isRecord(value) || typeof value.isValid !== "boolean") {
    return false;
  }
  if (!value.isValid && !hasNonEmptyReason(value.invalidReason)) {
    return false;
  }
  return true;
}

function isSettleResponse(value: unknown): value is SettleResponse {
  if (
    !isRecord(value) ||
    typeof value.success !== "boolean" ||
    typeof value.transaction !== "string" ||
    typeof value.network !== "string"
  ) {
    return false;
  }
  if (!value.success && !hasNonEmptyReason(value.errorReason)) {
    return false;
  }
  if (value.success && value.transaction.trim().length === 0) {
    return false;
  }
  return true;
}

function isReservation(value: unknown): value is SettlementReservation {
  if (!isRecord(value) || typeof value.status !== "string") {
    return false;
  }
  if (value.status === "reserved") {
    return isNonEmptyBoundedString(value.reservationId, 256);
  }
  if (value.status === "rejected") {
    return (
      [
        "settlement_unverified",
        "settlement_duplicate",
        "settlement_replay",
      ].includes(String(value.code)) && hasNonEmptyReason(value.reason)
    );
  }
  return (
    value.status === "unavailable" &&
    ["settlement_state_unavailable", "settlement_outcome_unknown"].includes(
      String(value.code),
    ) &&
    hasNonEmptyReason(value.reason)
  );
}

function isFinalization(value: unknown): value is SettlementFinalization {
  if (!isRecord(value) || typeof value.status !== "string") {
    return false;
  }
  if (value.status === "recorded") {
    return true;
  }
  return (
    value.status === "unavailable" &&
    ["settlement_outcome_unknown", "settlement_state_unavailable"].includes(
      String(value.code),
    ) &&
    hasNonEmptyReason(value.reason)
  );
}

function parseEvaluationRequest(
  input: unknown,
): ParseResult<NeuroChainEvaluationRequest> {
  if (!isRecord(input) || !hasExactKeys(input, EVALUATION_REQUEST_KEYS)) {
    return {
      ok: false,
      reason: "evaluation request must use the strict Rust boundary envelope",
    };
  }
  if (
    input.schema_version !== 1 ||
    input.message_type !== "evaluation_request" ||
    input.operation !== "plan_and_evaluate" ||
    typeof input.network !== "string" ||
    !isSupportedNetwork(input.network) ||
    !isNonEmptyBoundedString(input.request_id, 128) ||
    !isNonEmptyBoundedString(input.resource_id, 256) ||
    !isNonEmptyBoundedString(input.intent_text, 4096)
  ) {
    return {
      ok: false,
      reason: "evaluation request failed the versioned Rust boundary rules",
    };
  }
  return { ok: true, value: input as unknown as NeuroChainEvaluationRequest };
}

function parseActionPlan(input: unknown): input is Readonly<Record<string, unknown>> {
  if (!isRecord(input)) {
    return false;
  }
  const actualKeys = Object.keys(input).sort();
  if (actualKeys.some((key) => !ACTION_PLAN_KEYS.includes(key))) {
    return false;
  }
  if (
    input.schema_version !== 1 ||
    !Array.isArray(input.actions) ||
    input.actions.length < 1 ||
    input.actions.length > 64
  ) {
    return false;
  }
  if (
    input.actions.some(
      (action) =>
        !isRecord(action) || !isNonEmptyBoundedString(action.kind, 128),
    )
  ) {
    return false;
  }
  if (
    input.warnings !== undefined &&
    (!Array.isArray(input.warnings) ||
      input.warnings.some((warning) => typeof warning !== "string"))
  ) {
    return false;
  }
  if (input.source !== undefined && typeof input.source !== "string") {
    return false;
  }
  try {
    return Buffer.byteLength(JSON.stringify(input), "utf8") <= 65_536;
  } catch {
    return false;
  }
}

function hasNoAuthority(value: unknown): value is NeuroChainAuthorityGrants {
  return (
    isRecord(value) &&
    hasExactKeys(value, AUTHORITY_GRANT_KEYS) &&
    value.payment_verification === false &&
    value.payment_settlement === false &&
    value.guardrail_override === false &&
    value.wallet_signing === false &&
    value.stellar_submission === false
  );
}

function parseEvaluationResponse(
  input: unknown,
): ParseResult<NeuroChainEvaluationResponse> {
  if (!isRecord(input) || !hasExactKeys(input, EVALUATION_RESPONSE_KEYS)) {
    return {
      ok: false,
      reason: "evaluation response must use the strict Rust boundary envelope",
    };
  }
  if (
    input.schema_version !== 1 ||
    input.message_type !== "evaluation_response" ||
    !isNonEmptyBoundedString(input.request_id, 128) ||
    !isNonEmptyBoundedString(input.reason_code, 128) ||
    !parseActionPlan(input.action_plan) ||
    typeof input.action_plan_hash !== "string" ||
    !/^[0-9a-f]{64}$/.test(input.action_plan_hash) ||
    !hasNoAuthority(input.authority_grants) ||
    input.underlying_action_submit_allowed !== false
  ) {
    return {
      ok: false,
      reason: "evaluation response failed the versioned Rust boundary rules",
    };
  }

  const decisionValid =
    (input.decision === "approved" &&
      input.exit_code === null &&
      input.reason_code === "approved") ||
    (input.decision === "requires_approval" &&
      input.exit_code === null &&
      input.reason_code === "approval_required") ||
    (input.decision === "blocked" &&
      [3, 4, 5].some((exitCode) => input.exit_code === exitCode));
  if (!decisionValid) {
    return {
      ok: false,
      reason: "evaluation decision and exit semantics do not match",
    };
  }
  return { ok: true, value: input as unknown as NeuroChainEvaluationResponse };
}

async function finalizeSettlement(
  state: SettlementStatePort,
  reservationId: string,
  outcome: SettlementOutcome,
): Promise<SettlementFinalization> {
  try {
    const finalized = await state.finalize(reservationId, outcome);
    if (isFinalization(finalized)) {
      return finalized;
    }
    return {
      status: "unavailable",
      code: "settlement_outcome_unknown",
      reason: "settlement state returned an invalid finalization result",
    };
  } catch {
    return {
      status: "unavailable",
      code: "settlement_outcome_unknown",
      reason: "settlement outcome could not be recorded durably",
    };
  }
}

export class PureFacilitatorHandlers {
  constructor(
    private readonly facilitator: FacilitatorPort,
    private readonly settlementState?: SettlementStatePort,
    private readonly evaluationPort?: NeuroChainEvaluationPort,
  ) {}

  handleSupported(): ServiceHandlerResult<SupportedResponse> {
    try {
      return result(
        "supported",
        "completed",
        "supported_ready",
        "upstream facilitator returned its registered capabilities",
        this.facilitator.getSupported(),
      );
    } catch {
      return result(
        "supported",
        "unavailable",
        "upstream_supported_unavailable",
        "upstream facilitator capabilities failed closed",
        null,
      );
    }
  }

  async handleVerify(
    input: unknown,
  ): Promise<ServiceHandlerResult<VerifyResponse>> {
    const parsed = parseFacilitatorRequest<VerifyRequest>(input);
    if (!parsed.ok) {
      return result("verify", "rejected", "request_invalid", parsed.reason, null);
    }
    if (!isSupportedNetwork(parsed.value.paymentRequirements.network)) {
      return result(
        "verify",
        "rejected",
        "unsupported_network",
        "the service supports only stellar:testnet and stellar:pubnet",
        null,
      );
    }
    try {
      const response = await this.facilitator.verify(
        parsed.value.paymentPayload,
        parsed.value.paymentRequirements,
      );
      if (!isVerifyResponse(response)) {
        return result(
          "verify",
          "unavailable",
          "upstream_verify_response_invalid",
          "upstream verify returned no stable fail-closed result",
          null,
        );
      }
      return result(
        "verify",
        "completed",
        response.isValid ? "verify_completed" : "verify_rejected",
        response.isValid
          ? "upstream facilitator completed verification"
          : (response.invalidReason ?? "verification_rejected"),
        response,
      );
    } catch {
      return result(
        "verify",
        "unavailable",
        "upstream_verify_unavailable",
        "upstream facilitator verify failed closed",
        null,
      );
    }
  }

  async handleSettle(
    input: unknown,
  ): Promise<ServiceHandlerResult<SettleResponse>> {
    const parsed = parseFacilitatorRequest<SettleRequest>(input);
    if (!parsed.ok) {
      return result("settle", "rejected", "request_invalid", parsed.reason, null);
    }
    if (!isSupportedNetwork(parsed.value.paymentRequirements.network)) {
      return result(
        "settle",
        "rejected",
        "unsupported_network",
        "the service supports only stellar:testnet and stellar:pubnet",
        null,
      );
    }
    if (this.settlementState === undefined) {
      return result(
        "settle",
        "unavailable",
        "settlement_state_unavailable",
        "settlement requires an atomic persistent admission adapter",
        null,
      );
    }

    let reservation: SettlementReservation;
    try {
      const candidate = await this.settlementState.reserve(parsed.value);
      if (!isReservation(candidate)) {
        return result(
          "settle",
          "unavailable",
          "settlement_state_response_invalid",
          "settlement state returned an invalid reservation result",
          null,
        );
      }
      reservation = candidate;
    } catch {
      return result(
        "settle",
        "unavailable",
        "settlement_state_unavailable",
        "settlement admission state failed closed",
        null,
      );
    }

    if (reservation.status !== "reserved") {
      return result(
        "settle",
        reservation.status,
        reservation.code,
        reservation.reason,
        null,
      );
    }

    let response: SettleResponse;
    try {
      response = await this.facilitator.settle(
        parsed.value.paymentPayload,
        parsed.value.paymentRequirements,
      );
    } catch {
      const finalized = await finalizeSettlement(
        this.settlementState,
        reservation.reservationId,
        { status: "upstream_error", code: "upstream_settle_unavailable" },
      );
      if (finalized.status !== "recorded") {
        return result(
          "settle",
          "unavailable",
          finalized.code,
          finalized.reason,
          null,
        );
      }
      return result(
        "settle",
        "unavailable",
        "upstream_settle_unavailable",
        "upstream facilitator settle failed closed",
        null,
      );
    }

    if (!isSettleResponse(response)) {
      const finalized = await finalizeSettlement(
        this.settlementState,
        reservation.reservationId,
        {
          status: "upstream_error",
          code: "upstream_settle_response_invalid",
        },
      );
      if (finalized.status !== "recorded") {
        return result(
          "settle",
          "unavailable",
          finalized.code,
          finalized.reason,
          null,
        );
      }
      return result(
        "settle",
        "unavailable",
        "upstream_settle_response_invalid",
        "upstream settle returned no stable fail-closed result",
        null,
      );
    }

    const finalized = await finalizeSettlement(
      this.settlementState,
      reservation.reservationId,
      { status: "upstream_response", response },
    );
    if (finalized.status !== "recorded") {
      return result(
        "settle",
        "unavailable",
        finalized.code,
        finalized.reason,
        null,
      );
    }
    return result(
      "settle",
      "completed",
      response.success ? "settle_completed" : "settle_rejected",
      response.success
        ? "upstream facilitator completed settlement"
        : (response.errorReason ?? "settlement_rejected"),
      response,
    );
  }

  async handleEvaluation(
    input: unknown,
  ): Promise<ServiceHandlerResult<NeuroChainEvaluationResponse>> {
    const request = parseEvaluationRequest(input);
    if (!request.ok) {
      return result(
        "evaluation",
        "rejected",
        "evaluation_request_invalid",
        request.reason,
        null,
      );
    }
    if (this.evaluationPort === undefined) {
      return result(
        "evaluation",
        "unavailable",
        "evaluation_unavailable",
        "the NeuroChain evaluation adapter is not configured",
        null,
      );
    }

    let rawResponse: unknown;
    try {
      rawResponse = await this.evaluationPort.evaluate(request.value);
    } catch {
      return result(
        "evaluation",
        "unavailable",
        "evaluation_unavailable",
        "NeuroChain evaluation failed closed",
        null,
      );
    }
    const response = parseEvaluationResponse(rawResponse);
    if (!response.ok) {
      return result(
        "evaluation",
        "rejected",
        "evaluation_response_invalid",
        response.reason,
        null,
      );
    }
    if (response.value.request_id !== request.value.request_id) {
      return result(
        "evaluation",
        "rejected",
        "evaluation_request_id_mismatch",
        "NeuroChain response request_id does not match the request",
        null,
      );
    }
    return result(
      "evaluation",
      "completed",
      "evaluation_completed",
      "versioned NeuroChain guardrail response passed the no-authority boundary",
      response.value,
    );
  }
}
