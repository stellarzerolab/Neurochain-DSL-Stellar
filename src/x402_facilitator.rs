use std::env;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::x402_store::{
    X402ChallengeRecord, X402ChallengeStore, X402FinalizeOutcome, X402StellarChallenge,
};

pub const X402_FACILITATOR_PROTOCOL_VERSION: u8 = 2;
const MIN_FACILITATOR_TIMEOUT_MS: u64 = 100;
const MAX_FACILITATOR_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X402FacilitatorConfig {
    pub endpoint: String,
    pub network: String,
    pub asset_contract: String,
    pub receiver: String,
    pub timeout_ms: u64,
}

impl X402FacilitatorConfig {
    pub fn validate(
        endpoint: &str,
        network: &str,
        asset_contract: &str,
        receiver: &str,
        timeout_ms: u64,
    ) -> Result<Self, X402FacilitatorTransportError> {
        let endpoint = endpoint.trim().trim_end_matches('/');
        let parsed = reqwest::Url::parse(endpoint).map_err(|err| {
            X402FacilitatorTransportError::InvalidConfiguration(format!(
                "invalid facilitator endpoint: {err}"
            ))
        })?;
        if parsed.scheme() != "https"
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
            || parsed.query().is_some()
            || parsed.fragment().is_some()
        {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                "facilitator endpoint must be a credential-free HTTPS base URL without query or fragment"
                    .to_string(),
            ));
        }

        let network = network.trim();
        if !matches!(network, "stellar:testnet" | "stellar:pubnet") {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                "facilitator network must be stellar:testnet or stellar:pubnet".to_string(),
            ));
        }
        if !is_stellar_strkey(asset_contract.trim(), &['C']) {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                "asset_contract must be a Stellar contract StrKey".to_string(),
            ));
        }
        if !is_stellar_strkey(receiver.trim(), &['G', 'C', 'M']) {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                "receiver must be a Stellar account, contract, or muxed-account StrKey".to_string(),
            ));
        }
        if !(MIN_FACILITATOR_TIMEOUT_MS..=MAX_FACILITATOR_TIMEOUT_MS).contains(&timeout_ms) {
            return Err(X402FacilitatorTransportError::InvalidConfiguration(
                format!(
                    "facilitator timeout must be between {MIN_FACILITATOR_TIMEOUT_MS} and {MAX_FACILITATOR_TIMEOUT_MS} milliseconds"
                ),
            ));
        }

        Ok(Self {
            endpoint: endpoint.to_string(),
            network: network.to_string(),
            asset_contract: asset_contract.trim().to_string(),
            receiver: receiver.trim().to_string(),
            timeout_ms,
        })
    }

    pub fn verify_url(&self) -> String {
        format!("{}/verify", self.endpoint)
    }

    pub fn settle_url(&self) -> String {
        format!("{}/settle", self.endpoint)
    }

    pub fn supported_url(&self) -> String {
        format!("{}/supported", self.endpoint)
    }

    pub fn validate_supported(
        &self,
        response: &X402FacilitatorSupportedResponse,
    ) -> Result<(), X402FacilitatorTransportError> {
        let supported = response.kinds.iter().any(|kind| {
            kind.x402_version == X402_FACILITATOR_PROTOCOL_VERSION
                && matches!(kind.scheme.as_str(), "exact" | "exact-v2")
                && kind.network == self.network
                && kind
                    .asset_contracts
                    .iter()
                    .any(|asset| asset == &self.asset_contract)
        });
        if supported {
            Ok(())
        } else {
            Err(X402FacilitatorTransportError::InvalidConfiguration(
                "facilitator does not advertise x402 v2 exact support for the configured Stellar network and asset"
                    .to_string(),
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct X402FacilitatorRequest {
    pub x402_version: u8,
    pub payment_payload: Value,
    pub payment_requirements: Value,
    pub idempotency_key: String,
    pub network: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct X402FacilitatorVerifyResponse {
    pub is_valid: bool,
    pub invalid_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct X402FacilitatorSettleResponse {
    pub success: bool,
    pub transaction_hash: Option<String>,
    pub error_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct X402FacilitatorSupportedKind {
    pub x402_version: u8,
    pub scheme: String,
    pub network: String,
    pub asset_contracts: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct X402FacilitatorSupportedResponse {
    pub kinds: Vec<X402FacilitatorSupportedKind>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X402FacilitatorTransportError {
    InvalidConfiguration(String),
    Unavailable(String),
    Timeout,
    InvalidResponse(String),
}

pub trait X402FacilitatorTransport {
    fn transport_kind(&self) -> &'static str;
    fn supported(&self) -> Result<X402FacilitatorSupportedResponse, X402FacilitatorTransportError>;
    fn verify(
        &self,
        request: &X402FacilitatorRequest,
    ) -> Result<X402FacilitatorVerifyResponse, X402FacilitatorTransportError>;
    fn settle(
        &self,
        request: &X402FacilitatorRequest,
    ) -> Result<X402FacilitatorSettleResponse, X402FacilitatorTransportError>;
}

pub fn verify_with_capability_handshake<T: X402FacilitatorTransport>(
    config: &X402FacilitatorConfig,
    transport: &T,
    request: &X402FacilitatorRequest,
) -> Result<X402FacilitatorVerifyResponse, X402FacilitatorTransportError> {
    if request.network != config.network {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "facilitator request network must match configured network".to_string(),
        ));
    }

    let supported = transport.supported()?;
    config.validate_supported(&supported)?;
    transport.verify(request)
}

pub fn facilitator_request_from_adapter(
    envelope: &Value,
    expected_operation: &str,
) -> Result<X402FacilitatorRequest, X402FacilitatorTransportError> {
    let operation = required_adapter_string(envelope, "operation")?;
    if operation != expected_operation {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            format!("expected adapter operation {expected_operation:?}, got {operation:?}"),
        ));
    }

    let payment_payload = envelope
        .get("payment_payload")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or_else(|| {
            X402FacilitatorTransportError::InvalidConfiguration(
                "payment_payload object is required".to_string(),
            )
        })?;
    let payment_requirements = envelope
        .get("payment_requirements")
        .filter(|value| value.is_object())
        .cloned()
        .ok_or_else(|| {
            X402FacilitatorTransportError::InvalidConfiguration(
                "payment_requirements object is required".to_string(),
            )
        })?;
    let x402_version = payment_payload
        .get("x402_version")
        .and_then(Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or_else(|| {
            X402FacilitatorTransportError::InvalidConfiguration(
                "payment_payload.x402_version must be an unsigned byte".to_string(),
            )
        })?;
    if x402_version != X402_FACILITATOR_PROTOCOL_VERSION {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            format!("unsupported x402 version {x402_version}; expected 2"),
        ));
    }

    let idempotency_key = required_adapter_string(envelope, "idempotency_key")?.to_string();
    let network = required_adapter_string(envelope, "network")?.to_string();
    if payment_requirements.get("network").and_then(Value::as_str) != Some(network.as_str()) {
        return Err(X402FacilitatorTransportError::InvalidConfiguration(
            "adapter network must match payment_requirements.network".to_string(),
        ));
    }

    Ok(X402FacilitatorRequest {
        x402_version,
        payment_payload,
        payment_requirements,
        idempotency_key,
        network,
    })
}

pub fn verify_response_to_adapter(
    request: &X402FacilitatorRequest,
    response: &X402FacilitatorVerifyResponse,
) -> Value {
    serde_json::json!({
        "schema_version": 1,
        "operation": "verify",
        "outcome": if response.is_valid { "verified" } else { "rejected" },
        "payment_payload": request.payment_payload,
        "payment_requirements": request.payment_requirements,
        "idempotency_key": request.idempotency_key,
        "network": request.network,
        "verification": {
            "is_valid": response.is_valid,
            "invalid_reason": response.invalid_reason,
        },
        "underlying_action_submit_allowed": false,
    })
}

pub fn settle_response_to_adapter(
    request: &X402FacilitatorRequest,
    response: &X402FacilitatorSettleResponse,
) -> Value {
    serde_json::json!({
        "schema_version": 1,
        "operation": "settle",
        "outcome": if response.success { "settled" } else { "rejected" },
        "payment_payload": request.payment_payload,
        "payment_requirements": request.payment_requirements,
        "idempotency_key": request.idempotency_key,
        "network": request.network,
        "verification": {
            "is_valid": true,
            "invalid_reason": null,
        },
        "settlement": {
            "success": response.success,
            "transaction_hash": response.transaction_hash,
            "error_reason": response.error_reason,
        },
        "underlying_action_submit_allowed": false,
    })
}

fn required_adapter_string<'a>(
    envelope: &'a Value,
    field: &str,
) -> Result<&'a str, X402FacilitatorTransportError> {
    envelope
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            X402FacilitatorTransportError::InvalidConfiguration(format!(
                "{field} must be a non-empty string"
            ))
        })
}

fn is_stellar_strkey(value: &str, prefixes: &[char]) -> bool {
    value.len() == 56
        && value
            .chars()
            .next()
            .is_some_and(|first| prefixes.contains(&first))
        && value
            .chars()
            .all(|character| matches!(character, 'A'..='Z' | '2'..='7'))
}

#[derive(Debug, Clone)]
pub enum X402PaymentVerification {
    Finalized {
        challenge_id: String,
        challenge: X402StellarChallenge,
    },
    ReplayBlocked {
        challenge_id: String,
        challenge: X402StellarChallenge,
    },
    Expired {
        challenge_id: String,
        challenge: X402StellarChallenge,
    },
    InvalidPayment,
}

pub trait X402PaymentVerifier {
    fn verifier_kind(&self) -> &'static str;
    fn boundary_kind(&self) -> &'static str;
    fn create_challenge(
        &self,
        store: &mut dyn X402ChallengeStore,
    ) -> Result<X402ChallengeRecord, String>;
    fn verify_and_finalize(
        &self,
        payment_signature: &str,
        store: &mut dyn X402ChallengeStore,
    ) -> Result<X402PaymentVerification, String>;
}

#[derive(Debug, Default)]
struct MockX402PaymentVerifier;

#[derive(Debug, Default)]
struct FacilitatorX402PaymentVerifier;

#[derive(Debug)]
struct UnavailableX402PaymentVerifier {
    reason: String,
}

impl X402PaymentVerifier for MockX402PaymentVerifier {
    fn verifier_kind(&self) -> &'static str {
        "mock"
    }

    fn boundary_kind(&self) -> &'static str {
        "mock_header_store"
    }

    fn create_challenge(
        &self,
        store: &mut dyn X402ChallengeStore,
    ) -> Result<X402ChallengeRecord, String> {
        store.create_challenge()
    }

    fn verify_and_finalize(
        &self,
        payment_signature: &str,
        store: &mut dyn X402ChallengeStore,
    ) -> Result<X402PaymentVerification, String> {
        let Some(challenge_id) =
            mock_challenge_from_signature(payment_signature).map(str::to_string)
        else {
            return Ok(X402PaymentVerification::InvalidPayment);
        };

        let verification = match store.begin_finalize(&challenge_id)? {
            X402FinalizeOutcome::Finalized(challenge) => X402PaymentVerification::Finalized {
                challenge_id,
                challenge,
            },
            X402FinalizeOutcome::ReplayBlocked(challenge) => {
                X402PaymentVerification::ReplayBlocked {
                    challenge_id,
                    challenge,
                }
            }
            X402FinalizeOutcome::Expired(challenge) => X402PaymentVerification::Expired {
                challenge_id,
                challenge,
            },
            X402FinalizeOutcome::UnknownChallenge => X402PaymentVerification::InvalidPayment,
        };

        Ok(verification)
    }
}

impl X402PaymentVerifier for FacilitatorX402PaymentVerifier {
    fn verifier_kind(&self) -> &'static str {
        "facilitator"
    }

    fn boundary_kind(&self) -> &'static str {
        "facilitator_verify_settle"
    }

    fn create_challenge(
        &self,
        _store: &mut dyn X402ChallengeStore,
    ) -> Result<X402ChallengeRecord, String> {
        Err(facilitator_transport_unavailable())
    }

    fn verify_and_finalize(
        &self,
        _payment_signature: &str,
        _store: &mut dyn X402ChallengeStore,
    ) -> Result<X402PaymentVerification, String> {
        Err(facilitator_transport_unavailable())
    }
}

impl X402PaymentVerifier for UnavailableX402PaymentVerifier {
    fn verifier_kind(&self) -> &'static str {
        "unavailable"
    }

    fn boundary_kind(&self) -> &'static str {
        "facilitator_required"
    }

    fn create_challenge(
        &self,
        _store: &mut dyn X402ChallengeStore,
    ) -> Result<X402ChallengeRecord, String> {
        Err(self.reason.clone())
    }

    fn verify_and_finalize(
        &self,
        _payment_signature: &str,
        _store: &mut dyn X402ChallengeStore,
    ) -> Result<X402PaymentVerification, String> {
        Err(self.reason.clone())
    }
}

pub fn build_x402_payment_verifier() -> Box<dyn X402PaymentVerifier + Send + Sync> {
    let mode = env::var("NC_X402_STELLAR_VERIFIER")
        .unwrap_or_else(|_| "mock".to_string())
        .trim()
        .to_ascii_lowercase();

    select_x402_payment_verifier(&mode, x402_runtime_is_production())
}

fn select_x402_payment_verifier(
    mode: &str,
    production_runtime: bool,
) -> Box<dyn X402PaymentVerifier + Send + Sync> {
    match mode {
        "mock" if production_runtime => Box::new(UnavailableX402PaymentVerifier {
            reason:
                "mock x402 verifier is disabled in production; configure the facilitator verifier"
                    .to_string(),
        }),
        "mock" => Box::<MockX402PaymentVerifier>::default(),
        "facilitator" => Box::<FacilitatorX402PaymentVerifier>::default(),
        _ => Box::new(UnavailableX402PaymentVerifier {
            reason: format!(
                "unsupported x402 verifier mode {mode:?}; expected \"mock\" or \"facilitator\""
            ),
        }),
    }
}

fn facilitator_transport_unavailable() -> String {
    "facilitator x402 verifier is selected, but verify/settle transport is not implemented"
        .to_string()
}

fn mock_challenge_from_signature(signature: &str) -> Option<&str> {
    signature
        .trim()
        .strip_prefix("paid:")
        .map(str::trim)
        .filter(|challenge_id| !challenge_id.is_empty())
}

fn x402_runtime_is_production() -> bool {
    ["NC_ENV", "APP_ENV", "RUST_ENV"].iter().any(|key| {
        env::var(key)
            .ok()
            .map(|value| value.trim().eq_ignore_ascii_case("production"))
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Debug)]
    struct FakeFacilitatorTransport {
        calls: Mutex<Vec<&'static str>>,
        supported_error: Option<X402FacilitatorTransportError>,
    }

    impl FakeFacilitatorTransport {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                supported_error: None,
            }
        }

        fn failing_supported(error: X402FacilitatorTransportError) -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
                supported_error: Some(error),
            }
        }
    }

    impl X402FacilitatorTransport for FakeFacilitatorTransport {
        fn transport_kind(&self) -> &'static str {
            "offline_fake"
        }

        fn supported(
            &self,
        ) -> Result<X402FacilitatorSupportedResponse, X402FacilitatorTransportError> {
            self.calls.lock().unwrap().push("supported");
            if let Some(error) = &self.supported_error {
                return Err(error.clone());
            }
            Ok(serde_json::from_str(include_str!(
                "../examples/x402_facilitator_adapter/supported_stellar_exact_v2.json"
            ))
            .unwrap())
        }

        fn verify(
            &self,
            request: &X402FacilitatorRequest,
        ) -> Result<X402FacilitatorVerifyResponse, X402FacilitatorTransportError> {
            self.calls.lock().unwrap().push("verify");
            if request.x402_version != X402_FACILITATOR_PROTOCOL_VERSION {
                return Err(X402FacilitatorTransportError::InvalidConfiguration(
                    "unsupported x402 version".to_string(),
                ));
            }
            Ok(X402FacilitatorVerifyResponse {
                is_valid: true,
                invalid_reason: None,
            })
        }

        fn settle(
            &self,
            request: &X402FacilitatorRequest,
        ) -> Result<X402FacilitatorSettleResponse, X402FacilitatorTransportError> {
            self.calls.lock().unwrap().push("settle");
            if request.idempotency_key.trim().is_empty() {
                return Err(X402FacilitatorTransportError::InvalidConfiguration(
                    "idempotency key is required".to_string(),
                ));
            }
            Ok(X402FacilitatorSettleResponse {
                success: true,
                transaction_hash: Some("fixture:payment-transaction".to_string()),
                error_reason: None,
            })
        }
    }

    #[derive(Debug, Default)]
    struct TestChallengeStore {
        created: bool,
        finalized: bool,
    }

    impl X402ChallengeStore for TestChallengeStore {
        fn store_kind(&self) -> &'static str {
            "test"
        }

        fn create_challenge(&mut self) -> Result<X402ChallengeRecord, String> {
            self.created = true;
            Ok(X402ChallengeRecord {
                challenge_id: "x402s0001".to_string(),
                challenge: X402StellarChallenge {
                    created_at: 1,
                    expires_at: 300,
                    finalized: false,
                    finalized_at: None,
                    payment_state: "payment_required".to_string(),
                },
            })
        }

        fn begin_finalize(&mut self, challenge_id: &str) -> Result<X402FinalizeOutcome, String> {
            self.finalized = true;
            if challenge_id == "x402s0001" {
                Ok(X402FinalizeOutcome::Finalized(X402StellarChallenge {
                    created_at: 1,
                    expires_at: 300,
                    finalized: true,
                    finalized_at: Some(2),
                    payment_state: "finalized".to_string(),
                }))
            } else {
                Ok(X402FinalizeOutcome::UnknownChallenge)
            }
        }
    }

    #[test]
    fn selects_mock_verifier_only_for_non_production_runtime() {
        let verifier = select_x402_payment_verifier("mock", false);
        assert_eq!(verifier.verifier_kind(), "mock");
        assert_eq!(verifier.boundary_kind(), "mock_header_store");

        let mut store = TestChallengeStore::default();
        let challenge = verifier.create_challenge(&mut store).unwrap();
        assert_eq!(challenge.challenge_id, "x402s0001");
        assert!(store.created);

        let verification = verifier
            .verify_and_finalize("paid:x402s0001", &mut store)
            .unwrap();
        assert!(matches!(
            verification,
            X402PaymentVerification::Finalized { .. }
        ));
        assert!(store.finalized);
    }

    #[test]
    fn disables_mock_verifier_for_production_runtime() {
        let verifier = select_x402_payment_verifier("mock", true);
        assert_eq!(verifier.verifier_kind(), "unavailable");
        assert_eq!(verifier.boundary_kind(), "facilitator_required");

        let mut store = TestChallengeStore::default();
        let err = verifier.create_challenge(&mut store).unwrap_err();
        assert!(err.contains("mock x402 verifier is disabled in production"));
        assert!(!store.created);

        let err = verifier
            .verify_and_finalize("paid:x402s0001", &mut store)
            .unwrap_err();
        assert!(err.contains("configure the facilitator verifier"));
        assert!(!store.finalized);
    }

    #[test]
    fn selects_facilitator_as_explicit_fail_closed_boundary() {
        let verifier = select_x402_payment_verifier("facilitator", false);
        assert_eq!(verifier.verifier_kind(), "facilitator");
        assert_eq!(verifier.boundary_kind(), "facilitator_verify_settle");

        let mut store = TestChallengeStore::default();
        let err = verifier.create_challenge(&mut store).unwrap_err();
        assert!(err.contains("verify/settle transport is not implemented"));
        assert!(!store.created);

        let err = verifier
            .verify_and_finalize("paid:x402s0001", &mut store)
            .unwrap_err();
        assert!(err.contains("facilitator x402 verifier is selected"));
        assert!(!store.finalized);
    }

    #[test]
    fn unsupported_verifier_mode_is_unavailable() {
        let verifier = select_x402_payment_verifier("wallet", false);
        assert_eq!(verifier.verifier_kind(), "unavailable");
        assert_eq!(verifier.boundary_kind(), "facilitator_required");

        let mut store = TestChallengeStore::default();
        let err = verifier.create_challenge(&mut store).unwrap_err();
        assert!(err.contains("unsupported x402 verifier mode"));
        assert!(err.contains("mock"));
        assert!(err.contains("facilitator"));
        assert!(!store.created);
    }

    #[test]
    fn facilitator_transport_port_keeps_verify_and_settle_separate_offline() {
        let transport = FakeFacilitatorTransport::new();
        assert_eq!(transport.transport_kind(), "offline_fake");
        let request = X402FacilitatorRequest {
            x402_version: X402_FACILITATOR_PROTOCOL_VERSION,
            payment_payload: serde_json::json!({"payload_ref": "fixture:payment"}),
            payment_requirements: serde_json::json!({
                "scheme": "exact",
                "network": "stellar:testnet"
            }),
            idempotency_key: "fixture-request-0001".to_string(),
            network: "stellar:testnet".to_string(),
        };

        let verified = transport.verify(&request).unwrap();
        assert!(verified.is_valid);
        assert_eq!(transport.calls.lock().unwrap().as_slice(), ["verify"]);

        let settled = transport.settle(&request).unwrap();
        assert!(settled.success);
        assert_eq!(
            settled.transaction_hash.as_deref(),
            Some("fixture:payment-transaction")
        );
        assert_eq!(
            transport.calls.lock().unwrap().as_slice(),
            ["verify", "settle"]
        );
    }

    #[test]
    fn facilitator_transport_port_rejects_invalid_version_and_idempotency() {
        let transport = FakeFacilitatorTransport::new();
        let mut request = X402FacilitatorRequest {
            x402_version: 1,
            payment_payload: serde_json::json!({"payload_ref": "fixture:payment"}),
            payment_requirements: serde_json::json!({"scheme": "exact"}),
            idempotency_key: "fixture-request-0001".to_string(),
            network: "stellar:testnet".to_string(),
        };

        assert!(matches!(
            transport.verify(&request),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));

        request.x402_version = X402_FACILITATOR_PROTOCOL_VERSION;
        request.idempotency_key.clear();
        assert!(matches!(
            transport.settle(&request),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn adapter_fixtures_round_trip_through_offline_transport() {
        let transport = FakeFacilitatorTransport::new();
        let verify_fixture: Value = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/verify_valid.json"
        ))
        .unwrap();
        let verify_request = facilitator_request_from_adapter(&verify_fixture, "verify").unwrap();
        let verify_response = transport.verify(&verify_request).unwrap();
        assert_eq!(
            verify_response_to_adapter(&verify_request, &verify_response),
            verify_fixture
        );

        let settle_fixture: Value = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/settle_success.json"
        ))
        .unwrap();
        let settle_request = facilitator_request_from_adapter(&settle_fixture, "settle").unwrap();
        let settle_response = transport.settle(&settle_request).unwrap();
        assert_eq!(
            settle_response_to_adapter(&settle_request, &settle_response),
            settle_fixture
        );
        assert_eq!(
            transport.calls.lock().unwrap().as_slice(),
            ["verify", "settle"]
        );
    }

    #[test]
    fn adapter_mapping_rejects_operation_and_network_mismatch() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/verify_valid.json"
        ))
        .unwrap();
        assert!(matches!(
            facilitator_request_from_adapter(&fixture, "settle"),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));

        let mut mismatched = fixture;
        mismatched["network"] = Value::String("stellar:pubnet".to_string());
        assert!(matches!(
            facilitator_request_from_adapter(&mismatched, "verify"),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn facilitator_config_validates_stellar_transport_without_network_calls() {
        let config = X402FacilitatorConfig::validate(
            "https://channels.openzeppelin.com/x402/testnet/",
            "stellar:testnet",
            "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            5_000,
        )
        .unwrap();

        assert_eq!(
            config.endpoint,
            "https://channels.openzeppelin.com/x402/testnet"
        );
        assert_eq!(
            config.verify_url(),
            "https://channels.openzeppelin.com/x402/testnet/verify"
        );
        assert_eq!(
            config.settle_url(),
            "https://channels.openzeppelin.com/x402/testnet/settle"
        );
        assert_eq!(
            config.supported_url(),
            "https://channels.openzeppelin.com/x402/testnet/supported"
        );
    }

    #[test]
    fn facilitator_config_rejects_unsafe_or_incomplete_values() {
        let valid_asset = "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA";
        let valid_receiver = "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF";
        for (endpoint, network, asset, receiver, timeout_ms) in [
            (
                "http://facilitator.example",
                "stellar:testnet",
                valid_asset,
                valid_receiver,
                5_000,
            ),
            (
                "https://user:secret@facilitator.example",
                "stellar:testnet",
                valid_asset,
                valid_receiver,
                5_000,
            ),
            (
                "https://facilitator.example?network=testnet",
                "stellar:testnet",
                valid_asset,
                valid_receiver,
                5_000,
            ),
            (
                "https://facilitator.example",
                "stellar:mainnet",
                valid_asset,
                valid_receiver,
                5_000,
            ),
            (
                "https://facilitator.example",
                "stellar:testnet",
                "USDC",
                valid_receiver,
                5_000,
            ),
            (
                "https://facilitator.example",
                "stellar:testnet",
                valid_asset,
                "receiver",
                5_000,
            ),
            (
                "https://facilitator.example",
                "stellar:testnet",
                valid_asset,
                valid_receiver,
                50,
            ),
            (
                "https://facilitator.example",
                "stellar:testnet",
                valid_asset,
                valid_receiver,
                60_000,
            ),
        ] {
            assert!(matches!(
                X402FacilitatorConfig::validate(endpoint, network, asset, receiver, timeout_ms),
                Err(X402FacilitatorTransportError::InvalidConfiguration(_))
            ));
        }
    }

    #[test]
    fn facilitator_capability_handshake_accepts_configured_stellar_asset_offline() {
        let transport = FakeFacilitatorTransport::new();
        let config = X402FacilitatorConfig::validate(
            "https://channels.openzeppelin.com/x402/testnet",
            "stellar:testnet",
            "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            5_000,
        )
        .unwrap();
        let supported = transport.supported().unwrap();

        config.validate_supported(&supported).unwrap();
        assert_eq!(transport.calls.lock().unwrap().as_slice(), ["supported"]);
    }

    #[test]
    fn facilitator_capability_handshake_fails_closed_on_mismatch() {
        let config = X402FacilitatorConfig::validate(
            "https://facilitator.example",
            "stellar:pubnet",
            "CCW67TSZV3SSS2HXMBQ5JFGCKJNXKZM7UQUWUZPUTHXSTZLEO7SJMI75",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            5_000,
        )
        .unwrap();
        let testnet_only: X402FacilitatorSupportedResponse = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/supported_stellar_exact_v2.json"
        ))
        .unwrap();

        assert!(matches!(
            config.validate_supported(&testnet_only),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));

        let unsupported_version = X402FacilitatorSupportedResponse {
            kinds: vec![X402FacilitatorSupportedKind {
                x402_version: 1,
                scheme: "exact".to_string(),
                network: config.network.clone(),
                asset_contracts: vec![config.asset_contract.clone()],
            }],
        };
        assert!(matches!(
            config.validate_supported(&unsupported_version),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
    }

    #[test]
    fn facilitator_verify_requires_successful_capability_handshake() {
        let transport = FakeFacilitatorTransport::new();
        let config = X402FacilitatorConfig::validate(
            "https://channels.openzeppelin.com/x402/testnet",
            "stellar:testnet",
            "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            5_000,
        )
        .unwrap();
        let fixture: Value = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/verify_valid.json"
        ))
        .unwrap();
        let request = facilitator_request_from_adapter(&fixture, "verify").unwrap();

        let response = verify_with_capability_handshake(&config, &transport, &request).unwrap();

        assert!(response.is_valid);
        assert_eq!(
            transport.calls.lock().unwrap().as_slice(),
            ["supported", "verify"]
        );
    }

    #[test]
    fn facilitator_verify_fails_closed_before_verify_on_handshake_errors() {
        let config = X402FacilitatorConfig::validate(
            "https://channels.openzeppelin.com/x402/testnet",
            "stellar:testnet",
            "CBIELTK6YBZJU5UP2WWQEUCYKLPU6AUNZ2BQ4WWFEIE3USCIHMXQDAMA",
            "GAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAWHF",
            5_000,
        )
        .unwrap();
        let fixture: Value = serde_json::from_str(include_str!(
            "../examples/x402_facilitator_adapter/verify_valid.json"
        ))
        .unwrap();
        let request = facilitator_request_from_adapter(&fixture, "verify").unwrap();

        for error in [
            X402FacilitatorTransportError::Unavailable(
                "offline facilitator unavailable".to_string(),
            ),
            X402FacilitatorTransportError::Timeout,
        ] {
            let transport = FakeFacilitatorTransport::failing_supported(error.clone());
            assert_eq!(
                verify_with_capability_handshake(&config, &transport, &request),
                Err(error)
            );
            assert_eq!(transport.calls.lock().unwrap().as_slice(), ["supported"]);
        }

        let mut mismatched_request = request;
        mismatched_request.network = "stellar:pubnet".to_string();
        let transport = FakeFacilitatorTransport::new();
        assert!(matches!(
            verify_with_capability_handshake(&config, &transport, &mismatched_request),
            Err(X402FacilitatorTransportError::InvalidConfiguration(_))
        ));
        assert!(transport.calls.lock().unwrap().is_empty());
    }
}
