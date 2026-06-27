use neurochain_zk_guardrail_contract::{
    DecisionStatus, Digest32, ExitCode, PublicJournal, ReasonCode,
};

pub trait ProofVerifier {
    type Error;

    fn verify(
        &self,
        expected_image_id: &Digest32,
        journal_bytes: &[u8],
        proof: &[u8],
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullifierStoreError {
    AlreadyConsumed,
    Unavailable,
}

pub trait NullifierStore {
    fn consume_if_unused(&mut self, audit_nullifier: Digest32) -> Result<(), NullifierStoreError>;
}

#[derive(Debug, Clone, Copy)]
pub struct AttestationEnvelope<'a> {
    pub journal_bytes: &'a [u8],
    pub proof: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContractRejection {
    pub exit_code: ExitCode,
    pub reason_code: ReasonCode,
}

impl ContractRejection {
    const fn invalid_attestation() -> Self {
        Self {
            exit_code: ExitCode::ContractPolicy,
            reason_code: ReasonCode::InvalidAttestation,
        }
    }

    const fn replay() -> Self {
        Self {
            exit_code: ExitCode::ContractPolicy,
            reason_code: ReasonCode::Replay,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContractNextStep {
    EligibleForSeparateApprovalFlow,
    RequiresApproval,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptedAttestation {
    pub journal: PublicJournal,
}

impl AcceptedAttestation {
    pub fn next_step(self) -> ContractNextStep {
        match self.journal.decision_status {
            DecisionStatus::Approved => ContractNextStep::EligibleForSeparateApprovalFlow,
            DecisionStatus::RequiresApproval => ContractNextStep::RequiresApproval,
            DecisionStatus::Blocked => ContractNextStep::Blocked,
        }
    }
}

pub fn verify_and_consume<V: ProofVerifier, S: NullifierStore>(
    expected_image_id: Digest32,
    envelope: AttestationEnvelope<'_>,
    verifier: &V,
    nullifiers: &mut S,
) -> Result<AcceptedAttestation, ContractRejection> {
    if envelope.proof.is_empty() {
        return Err(ContractRejection::invalid_attestation());
    }

    verifier
        .verify(&expected_image_id, envelope.journal_bytes, envelope.proof)
        .map_err(|_| ContractRejection::invalid_attestation())?;

    let journal = PublicJournal::decode(envelope.journal_bytes)
        .map_err(|_| ContractRejection::invalid_attestation())?;
    if journal.evaluator_image_id != expected_image_id {
        return Err(ContractRejection::invalid_attestation());
    }

    nullifiers
        .consume_if_unused(journal.audit_nullifier)
        .map_err(|error| match error {
            NullifierStoreError::AlreadyConsumed => ContractRejection::replay(),
            NullifierStoreError::Unavailable => ContractRejection::invalid_attestation(),
        })?;

    Ok(AcceptedAttestation { journal })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use neurochain_zk_guardrail_contract::{ContractError, CONTRACT_VERSION};

    use super::*;

    const IMAGE_ID: Digest32 = [0x11; 32];
    const VALID_PROOF: &[u8] = b"fixture-risc-zero-proof";

    struct FixtureVerifier {
        accept: bool,
    }

    impl ProofVerifier for FixtureVerifier {
        type Error = ();

        fn verify(
            &self,
            _expected_image_id: &Digest32,
            _journal_bytes: &[u8],
            proof: &[u8],
        ) -> Result<(), Self::Error> {
            if self.accept && proof == VALID_PROOF {
                Ok(())
            } else {
                Err(())
            }
        }
    }

    #[derive(Default)]
    struct FixtureNullifierStore {
        consumed: HashSet<Digest32>,
        consume_attempts: usize,
        available: bool,
    }

    impl FixtureNullifierStore {
        fn available() -> Self {
            Self {
                available: true,
                ..Self::default()
            }
        }
    }

    impl NullifierStore for FixtureNullifierStore {
        fn consume_if_unused(
            &mut self,
            audit_nullifier: Digest32,
        ) -> Result<(), NullifierStoreError> {
            self.consume_attempts += 1;
            if !self.available {
                return Err(NullifierStoreError::Unavailable);
            }
            if !self.consumed.insert(audit_nullifier) {
                return Err(NullifierStoreError::AlreadyConsumed);
            }
            Ok(())
        }
    }

    fn journal(
        image_id: Digest32,
        decision_status: DecisionStatus,
        exit_code: ExitCode,
        reason_code: ReasonCode,
        requires_approval: bool,
        audit_nullifier: Digest32,
    ) -> PublicJournal {
        PublicJournal {
            contract_version: CONTRACT_VERSION,
            evaluator_image_id: image_id,
            action_plan_hash: [0x22; 32],
            policy_commitment: [0x33; 32],
            policy_version: 1,
            decision_status,
            exit_code,
            reason_code,
            requires_approval,
            audit_nullifier,
        }
    }

    fn encoded_journal(
        decision_status: DecisionStatus,
        exit_code: ExitCode,
        reason_code: ReasonCode,
        requires_approval: bool,
        nullifier: Digest32,
    ) -> Vec<u8> {
        journal(
            IMAGE_ID,
            decision_status,
            exit_code,
            reason_code,
            requires_approval,
            nullifier,
        )
        .encode()
        .unwrap()
    }

    fn envelope(bytes: &[u8]) -> AttestationEnvelope<'_> {
        AttestationEnvelope {
            journal_bytes: bytes,
            proof: VALID_PROOF,
        }
    }

    #[test]
    fn approved_attestation_consumes_once_and_replay_is_exit_4() {
        let bytes = encoded_journal(
            DecisionStatus::Approved,
            ExitCode::Passed,
            ReasonCode::Passed,
            false,
            [0x44; 32],
        );
        let verifier = FixtureVerifier { accept: true };
        let mut store = FixtureNullifierStore::available();

        let accepted = verify_and_consume(IMAGE_ID, envelope(&bytes), &verifier, &mut store)
            .expect("first use should succeed");
        assert_eq!(
            accepted.next_step(),
            ContractNextStep::EligibleForSeparateApprovalFlow
        );
        assert_eq!(store.consume_attempts, 1);

        assert_eq!(
            verify_and_consume(IMAGE_ID, envelope(&bytes), &verifier, &mut store),
            Err(ContractRejection {
                exit_code: ExitCode::ContractPolicy,
                reason_code: ReasonCode::Replay,
            })
        );
        assert_eq!(store.consume_attempts, 2);
    }

    #[test]
    fn invalid_proof_empty_proof_wrong_image_and_bad_journal_do_not_consume() {
        let bytes = encoded_journal(
            DecisionStatus::Approved,
            ExitCode::Passed,
            ReasonCode::Passed,
            false,
            [0x44; 32],
        );
        let invalid = ContractRejection {
            exit_code: ExitCode::ContractPolicy,
            reason_code: ReasonCode::InvalidAttestation,
        };
        let mut store = FixtureNullifierStore::available();

        assert_eq!(
            verify_and_consume(
                IMAGE_ID,
                envelope(&bytes),
                &FixtureVerifier { accept: false },
                &mut store,
            ),
            Err(invalid)
        );
        assert_eq!(
            verify_and_consume(
                IMAGE_ID,
                AttestationEnvelope {
                    journal_bytes: &bytes,
                    proof: &[],
                },
                &FixtureVerifier { accept: true },
                &mut store,
            ),
            Err(invalid)
        );

        let wrong_image = journal(
            [0x99; 32],
            DecisionStatus::Approved,
            ExitCode::Passed,
            ReasonCode::Passed,
            false,
            [0x45; 32],
        )
        .encode()
        .unwrap();
        assert_eq!(
            verify_and_consume(
                IMAGE_ID,
                envelope(&wrong_image),
                &FixtureVerifier { accept: true },
                &mut store,
            ),
            Err(invalid)
        );

        let mut malformed = bytes.clone();
        malformed[0] ^= 1;
        assert_eq!(
            PublicJournal::decode(&malformed),
            Err(ContractError::InvalidEncoding)
        );
        assert_eq!(
            verify_and_consume(
                IMAGE_ID,
                envelope(&malformed),
                &FixtureVerifier { accept: true },
                &mut store,
            ),
            Err(invalid)
        );
        assert_eq!(store.consume_attempts, 0);
    }

    #[test]
    fn unavailable_store_fails_closed_as_exit_4() {
        let bytes = encoded_journal(
            DecisionStatus::Approved,
            ExitCode::Passed,
            ReasonCode::Passed,
            false,
            [0x44; 32],
        );
        let mut store = FixtureNullifierStore::default();

        assert_eq!(
            verify_and_consume(
                IMAGE_ID,
                envelope(&bytes),
                &FixtureVerifier { accept: true },
                &mut store,
            ),
            Err(ContractRejection {
                exit_code: ExitCode::ContractPolicy,
                reason_code: ReasonCode::InvalidAttestation,
            })
        );
        assert_eq!(store.consume_attempts, 1);
        assert!(store.consumed.is_empty());
    }

    #[test]
    fn requires_approval_and_all_blocked_exits_remain_non_submit() {
        let verifier = FixtureVerifier { accept: true };
        let cases = [
            (
                DecisionStatus::RequiresApproval,
                ExitCode::Passed,
                ReasonCode::ApprovalThreshold,
                true,
                ContractNextStep::RequiresApproval,
            ),
            (
                DecisionStatus::Blocked,
                ExitCode::Allowlist,
                ReasonCode::Allowlist,
                false,
                ContractNextStep::Blocked,
            ),
            (
                DecisionStatus::Blocked,
                ExitCode::ContractPolicy,
                ReasonCode::ContractPolicy,
                false,
                ContractNextStep::Blocked,
            ),
            (
                DecisionStatus::Blocked,
                ExitCode::IntentSafety,
                ReasonCode::IntentSafety,
                false,
                ContractNextStep::Blocked,
            ),
        ];

        for (index, (decision, exit, reason, approval, expected)) in cases.into_iter().enumerate() {
            let mut nullifier = [0x50; 32];
            nullifier[0] = index as u8;
            let bytes = encoded_journal(decision, exit, reason, approval, nullifier);
            let mut store = FixtureNullifierStore::available();
            let accepted =
                verify_and_consume(IMAGE_ID, envelope(&bytes), &verifier, &mut store).unwrap();
            assert_eq!(accepted.next_step(), expected);
        }
    }
}
