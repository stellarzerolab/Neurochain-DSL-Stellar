use neurochain_zk_guardrail_contract::{ContractError, DecisionStatus, Digest32, PublicJournal};

pub trait ReceiptVerifier {
    type Error;

    fn verify(
        &self,
        expected_image_id: &Digest32,
        journal_bytes: &[u8],
        receipt_seal: &[u8],
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Copy)]
pub struct ReceiptEnvelope<'a> {
    pub journal_bytes: &'a [u8],
    pub receipt_seal: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostVerificationError {
    EmptyReceiptSeal,
    ReceiptRejected,
    InvalidJournal(ContractError),
    ImageIdMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifiedNextStep {
    EligibleForSeparateApprovalFlow,
    RequiresApproval,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifiedAttestation {
    pub journal: PublicJournal,
}

impl VerifiedAttestation {
    pub fn next_step(self) -> VerifiedNextStep {
        match self.journal.decision_status {
            DecisionStatus::Approved => VerifiedNextStep::EligibleForSeparateApprovalFlow,
            DecisionStatus::RequiresApproval => VerifiedNextStep::RequiresApproval,
            DecisionStatus::Blocked => VerifiedNextStep::Blocked,
        }
    }
}

pub fn verify_attestation<V: ReceiptVerifier>(
    expected_image_id: Digest32,
    envelope: ReceiptEnvelope<'_>,
    verifier: &V,
) -> Result<VerifiedAttestation, HostVerificationError> {
    if envelope.receipt_seal.is_empty() {
        return Err(HostVerificationError::EmptyReceiptSeal);
    }

    verifier
        .verify(
            &expected_image_id,
            envelope.journal_bytes,
            envelope.receipt_seal,
        )
        .map_err(|_| HostVerificationError::ReceiptRejected)?;

    let journal = PublicJournal::decode(envelope.journal_bytes)
        .map_err(HostVerificationError::InvalidJournal)?;
    if journal.evaluator_image_id != expected_image_id {
        return Err(HostVerificationError::ImageIdMismatch);
    }

    Ok(VerifiedAttestation { journal })
}

#[cfg(test)]
mod tests {
    use neurochain_zk_guardrail_contract::{ExitCode, ReasonCode, CONTRACT_VERSION};

    use super::*;

    const IMAGE_ID: Digest32 = [0x11; 32];
    const VALID_SEAL: &[u8] = b"fixture-receipt-seal";

    struct FixtureVerifier {
        accept: bool,
    }

    impl ReceiptVerifier for FixtureVerifier {
        type Error = ();

        fn verify(
            &self,
            _expected_image_id: &Digest32,
            _journal_bytes: &[u8],
            receipt_seal: &[u8],
        ) -> Result<(), Self::Error> {
            if self.accept && receipt_seal == VALID_SEAL {
                Ok(())
            } else {
                Err(())
            }
        }
    }

    fn journal(
        image_id: Digest32,
        decision_status: DecisionStatus,
        exit_code: ExitCode,
        reason_code: ReasonCode,
        requires_approval: bool,
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
            audit_nullifier: [0x44; 32],
        }
    }

    fn envelope(bytes: &[u8]) -> ReceiptEnvelope<'_> {
        ReceiptEnvelope {
            journal_bytes: bytes,
            receipt_seal: VALID_SEAL,
        }
    }

    #[test]
    fn verified_approved_is_only_eligible_for_separate_flow() {
        let bytes = journal(
            IMAGE_ID,
            DecisionStatus::Approved,
            ExitCode::Passed,
            ReasonCode::Passed,
            false,
        )
        .encode()
        .unwrap();
        let verified = verify_attestation(
            IMAGE_ID,
            envelope(&bytes),
            &FixtureVerifier { accept: true },
        )
        .unwrap();
        assert_eq!(
            verified.next_step(),
            VerifiedNextStep::EligibleForSeparateApprovalFlow
        );
    }

    #[test]
    fn requires_approval_and_blocked_remain_non_submit_boundaries() {
        let requires_approval = journal(
            IMAGE_ID,
            DecisionStatus::RequiresApproval,
            ExitCode::Passed,
            ReasonCode::ApprovalThreshold,
            true,
        )
        .encode()
        .unwrap();
        let verified = verify_attestation(
            IMAGE_ID,
            envelope(&requires_approval),
            &FixtureVerifier { accept: true },
        )
        .unwrap();
        assert_eq!(verified.next_step(), VerifiedNextStep::RequiresApproval);

        let blocked = journal(
            IMAGE_ID,
            DecisionStatus::Blocked,
            ExitCode::ContractPolicy,
            ReasonCode::ContractPolicy,
            false,
        )
        .encode()
        .unwrap();
        let verified = verify_attestation(
            IMAGE_ID,
            envelope(&blocked),
            &FixtureVerifier { accept: true },
        )
        .unwrap();
        assert_eq!(verified.next_step(), VerifiedNextStep::Blocked);
    }

    #[test]
    fn verifier_rejection_empty_seal_and_wrong_image_fail_closed() {
        let bytes = journal(
            IMAGE_ID,
            DecisionStatus::Approved,
            ExitCode::Passed,
            ReasonCode::Passed,
            false,
        )
        .encode()
        .unwrap();

        assert_eq!(
            verify_attestation(
                IMAGE_ID,
                envelope(&bytes),
                &FixtureVerifier { accept: false }
            ),
            Err(HostVerificationError::ReceiptRejected)
        );
        assert_eq!(
            verify_attestation(
                IMAGE_ID,
                ReceiptEnvelope {
                    journal_bytes: &bytes,
                    receipt_seal: &[],
                },
                &FixtureVerifier { accept: true }
            ),
            Err(HostVerificationError::EmptyReceiptSeal)
        );

        let wrong_image = journal(
            [0x99; 32],
            DecisionStatus::Approved,
            ExitCode::Passed,
            ReasonCode::Passed,
            false,
        )
        .encode()
        .unwrap();
        assert_eq!(
            verify_attestation(
                IMAGE_ID,
                envelope(&wrong_image),
                &FixtureVerifier { accept: true }
            ),
            Err(HostVerificationError::ImageIdMismatch)
        );
    }

    #[test]
    fn malformed_or_semantically_tampered_journal_is_rejected() {
        let mut bytes = journal(
            IMAGE_ID,
            DecisionStatus::Approved,
            ExitCode::Passed,
            ReasonCode::Passed,
            false,
        )
        .encode()
        .unwrap();
        bytes[0] ^= 1;
        assert_eq!(
            verify_attestation(
                IMAGE_ID,
                envelope(&bytes),
                &FixtureVerifier { accept: true }
            ),
            Err(HostVerificationError::InvalidJournal(
                ContractError::InvalidEncoding
            ))
        );
    }
}
