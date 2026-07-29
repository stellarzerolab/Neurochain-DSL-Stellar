use std::env;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::x402_store::{
    X402ChallengeRecord, X402ChallengeStore, X402FinalizeOutcome, X402StellarChallenge,
};

pub const X402_FACILITATOR_PROTOCOL_VERSION: u8 = 2;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum X402FacilitatorTransportError {
    InvalidConfiguration(String),
    Unavailable(String),
    Timeout,
    InvalidResponse(String),
}

pub trait X402FacilitatorTransport {
    fn transport_kind(&self) -> &'static str;
    fn verify(
        &self,
        request: &X402FacilitatorRequest,
    ) -> Result<X402FacilitatorVerifyResponse, X402FacilitatorTransportError>;
    fn settle(
        &self,
        request: &X402FacilitatorRequest,
    ) -> Result<X402FacilitatorSettleResponse, X402FacilitatorTransportError>;
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
    }

    impl FakeFacilitatorTransport {
        fn new() -> Self {
            Self {
                calls: Mutex::new(Vec::new()),
            }
        }
    }

    impl X402FacilitatorTransport for FakeFacilitatorTransport {
        fn transport_kind(&self) -> &'static str {
            "offline_fake"
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
}
