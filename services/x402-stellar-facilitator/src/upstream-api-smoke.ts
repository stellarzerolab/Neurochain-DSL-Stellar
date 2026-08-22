import { x402Facilitator } from "@x402/core/facilitator";
import { ExactStellarScheme } from "@x402/stellar/exact/facilitator";

const REQUIRED_STELLAR_METHODS = ["getExtra", "getSigners", "verify", "settle"] as const;
const REQUIRED_CORE_METHODS = ["register", "getSupported", "verify", "settle"] as const;

export interface OfflineUpstreamApiSnapshot {
  readonly coreConstructor: string;
  readonly stellarConstructor: string;
  readonly coreMethods: readonly string[];
  readonly stellarMethods: readonly string[];
  readonly emptySupportedKinds: number;
  readonly emptySupportedExtensions: number;
  readonly emptySupportedSignerNetworks: number;
  readonly networkAccessAllowed: false;
  readonly credentialUseAllowed: false;
  readonly signingAllowed: false;
  readonly settlementAllowed: false;
  readonly transactionSubmitAllowed: false;
  readonly actionPlanSubmitAllowed: false;
}

function requirePrototypeMethods(
  constructorName: string,
  prototype: object,
  requiredMethods: readonly string[],
): string[] {
  const available = new Set(Object.getOwnPropertyNames(prototype));
  for (const method of requiredMethods) {
    if (!available.has(method)) {
      throw new Error(`upstream_api_drift:${constructorName}.${method}`);
    }
  }
  return [...requiredMethods];
}

/**
 * Imports and inspects only the canonical upstream API surface.
 *
 * ExactStellarScheme is deliberately not instantiated because upstream
 * requires a facilitator signer. Supplying a signer, credential, RPC adapter,
 * or settlement authority is outside this offline bootstrap milestone.
 */
export function inspectOfflineUpstreamApi(): OfflineUpstreamApiSnapshot {
  const facilitator = new x402Facilitator();
  const supported = facilitator.getSupported();

  return Object.freeze({
    coreConstructor: x402Facilitator.name,
    stellarConstructor: ExactStellarScheme.name,
    coreMethods: Object.freeze(
      requirePrototypeMethods(
        x402Facilitator.name,
        x402Facilitator.prototype,
        REQUIRED_CORE_METHODS,
      ),
    ),
    stellarMethods: Object.freeze(
      requirePrototypeMethods(
        ExactStellarScheme.name,
        ExactStellarScheme.prototype,
        REQUIRED_STELLAR_METHODS,
      ),
    ),
    emptySupportedKinds: supported.kinds.length,
    emptySupportedExtensions: supported.extensions.length,
    emptySupportedSignerNetworks: Object.keys(supported.signers).length,
    networkAccessAllowed: false,
    credentialUseAllowed: false,
    signingAllowed: false,
    settlementAllowed: false,
    transactionSubmitAllowed: false,
    actionPlanSubmitAllowed: false,
  });
}
