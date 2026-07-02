#![no_std]

use neurochain_zk_guardrail_contract::{DecisionStatus, PublicJournal, PUBLIC_JOURNAL_ENCODED_LEN};
use risc0_interface::RiscZeroVerifierRouterClient;
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, Address, Bytes, BytesN,
    Env,
};

#[contracttype]
#[derive(Clone)]
struct Config {
    owner: Address,
    verifier_router: Address,
    evaluator_image_id: BytesN<32>,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Config,
    AuthorizedPolicy(BytesN<32>, u32),
    AuditNullifier(BytesN<32>),
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NextStep {
    EligibleForSeparateApprovalFlow,
    RequiresApproval,
    Blocked,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AcceptedAttestation {
    pub action_plan_hash: BytesN<32>,
    pub policy_commitment: BytesN<32>,
    pub policy_version: u32,
    pub decision_status: u32,
    pub exit_code: u32,
    pub reason_code: u32,
    pub requires_approval: bool,
    pub audit_nullifier: BytesN<32>,
    pub next_step: NextStep,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum AttestationError {
    NotInitialized = 1,
    InvalidAttestation = 2,
    Replay = 3,
}

#[contract]
pub struct NeuroChainZkGuardrail;

#[contractimpl]
impl NeuroChainZkGuardrail {
    pub fn __constructor(
        env: Env,
        owner: Address,
        verifier_router: Address,
        evaluator_image_id: BytesN<32>,
        initial_policy_commitment: BytesN<32>,
        initial_policy_version: u32,
    ) {
        if initial_policy_version == 0 {
            panic_with_error!(&env, AttestationError::InvalidAttestation);
        }
        env.storage().instance().set(
            &DataKey::Config,
            &Config {
                owner,
                verifier_router,
                evaluator_image_id,
            },
        );
        env.storage().instance().set(
            &DataKey::AuthorizedPolicy(initial_policy_commitment, initial_policy_version),
            &true,
        );
    }

    pub fn authorize_policy(
        env: Env,
        policy_commitment: BytesN<32>,
        policy_version: u32,
    ) -> Result<(), AttestationError> {
        let config = read_config(&env)?;
        config.owner.require_auth();
        if policy_version == 0 {
            return Err(AttestationError::InvalidAttestation);
        }
        env.storage().instance().set(
            &DataKey::AuthorizedPolicy(policy_commitment, policy_version),
            &true,
        );
        Ok(())
    }

    pub fn revoke_policy(
        env: Env,
        policy_commitment: BytesN<32>,
        policy_version: u32,
    ) -> Result<(), AttestationError> {
        let config = read_config(&env)?;
        config.owner.require_auth();
        env.storage().instance().remove(&DataKey::AuthorizedPolicy(
            policy_commitment,
            policy_version,
        ));
        Ok(())
    }

    pub fn is_policy_authorized(
        env: Env,
        policy_commitment: BytesN<32>,
        policy_version: u32,
    ) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::AuthorizedPolicy(
                policy_commitment,
                policy_version,
            ))
            .unwrap_or(false)
    }

    pub fn verify(
        env: Env,
        seal: Bytes,
        journal_bytes: Bytes,
    ) -> Result<AcceptedAttestation, AttestationError> {
        let journal = verify_attestation(&env, &seal, &journal_bytes)?;
        Ok(accepted_attestation(&env, journal))
    }

    pub fn verify_and_consume(
        env: Env,
        seal: Bytes,
        journal_bytes: Bytes,
    ) -> Result<AcceptedAttestation, AttestationError> {
        let config = read_config(&env)?;
        config.owner.require_auth();
        let journal = verify_attestation(&env, &seal, &journal_bytes)?;

        let audit_nullifier = BytesN::from_array(&env, &journal.audit_nullifier);
        let nullifier_key = DataKey::AuditNullifier(audit_nullifier.clone());
        if env.storage().persistent().has(&nullifier_key) {
            return Err(AttestationError::Replay);
        }

        env.storage().persistent().set(&nullifier_key, &true);
        let max_ttl = env.storage().max_ttl();
        env.storage()
            .persistent()
            .extend_ttl(&nullifier_key, max_ttl, max_ttl);

        Ok(accepted_attestation(&env, journal))
    }

    pub fn is_consumed(env: Env, audit_nullifier: BytesN<32>) -> bool {
        let key = DataKey::AuditNullifier(audit_nullifier);
        let consumed = env.storage().persistent().has(&key);
        if consumed {
            let max_ttl = env.storage().max_ttl();
            env.storage()
                .persistent()
                .extend_ttl(&key, max_ttl, max_ttl);
        }
        consumed
    }
}

fn verify_attestation(
    env: &Env,
    seal: &Bytes,
    journal_bytes: &Bytes,
) -> Result<PublicJournal, AttestationError> {
    if seal.len() <= 4 || journal_bytes.len() != PUBLIC_JOURNAL_ENCODED_LEN as u32 {
        return Err(AttestationError::InvalidAttestation);
    }

    let config = read_config(env)?;
    let journal_digest: BytesN<32> = env.crypto().sha256(journal_bytes).into();
    let verifier = RiscZeroVerifierRouterClient::new(env, &config.verifier_router);
    match verifier.try_verify(seal, &config.evaluator_image_id, &journal_digest) {
        Ok(Ok(())) => {}
        _ => return Err(AttestationError::InvalidAttestation),
    }

    let mut journal_raw = [0u8; PUBLIC_JOURNAL_ENCODED_LEN];
    journal_bytes.copy_into_slice(&mut journal_raw);
    let journal =
        PublicJournal::decode(&journal_raw).map_err(|_| AttestationError::InvalidAttestation)?;
    if journal.evaluator_image_id != config.evaluator_image_id.to_array() {
        return Err(AttestationError::InvalidAttestation);
    }
    let policy_key = DataKey::AuthorizedPolicy(
        BytesN::from_array(env, &journal.policy_commitment),
        journal.policy_version,
    );
    if !env.storage().instance().get(&policy_key).unwrap_or(false) {
        return Err(AttestationError::InvalidAttestation);
    }

    Ok(journal)
}

fn read_config(env: &Env) -> Result<Config, AttestationError> {
    let config = env
        .storage()
        .instance()
        .get(&DataKey::Config)
        .ok_or(AttestationError::NotInitialized)?;
    let max_ttl = env.storage().max_ttl();
    env.storage().instance().extend_ttl(max_ttl, max_ttl);
    Ok(config)
}

fn accepted_attestation(env: &Env, journal: PublicJournal) -> AcceptedAttestation {
    let next_step = match journal.decision_status {
        DecisionStatus::Approved => NextStep::EligibleForSeparateApprovalFlow,
        DecisionStatus::RequiresApproval => NextStep::RequiresApproval,
        DecisionStatus::Blocked => NextStep::Blocked,
    };
    AcceptedAttestation {
        action_plan_hash: BytesN::from_array(env, &journal.action_plan_hash),
        policy_commitment: BytesN::from_array(env, &journal.policy_commitment),
        policy_version: journal.policy_version,
        decision_status: journal.decision_status as u32,
        exit_code: journal.exit_code as u32,
        reason_code: journal.reason_code as u32,
        requires_approval: journal.requires_approval,
        audit_nullifier: BytesN::from_array(env, &journal.audit_nullifier),
        next_step,
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use mock_verifier::{RiscZeroMockVerifier, RiscZeroMockVerifierClient};
    use neurochain_zk_guardrail_contract::{Digest32, ExitCode, ReasonCode, CONTRACT_VERSION};
    use soroban_sdk::{crypto::Hash, testutils::Address as _, Env};

    use super::*;

    const IMAGE_ID: Digest32 = [0x11; 32];

    fn journal(
        decision_status: DecisionStatus,
        exit_code: ExitCode,
        reason_code: ReasonCode,
        requires_approval: bool,
        audit_nullifier: Digest32,
    ) -> PublicJournal {
        PublicJournal {
            contract_version: CONTRACT_VERSION,
            evaluator_image_id: IMAGE_ID,
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

    fn setup(
        journal: PublicJournal,
    ) -> (
        Env,
        NeuroChainZkGuardrailClient<'static>,
        RiscZeroMockVerifierClient<'static>,
        Bytes,
        BytesN<32>,
    ) {
        setup_with_policy(journal, [0x33; 32], 1)
    }

    fn setup_with_policy(
        journal: PublicJournal,
        policy_commitment: Digest32,
        policy_version: u32,
    ) -> (
        Env,
        NeuroChainZkGuardrailClient<'static>,
        RiscZeroMockVerifierClient<'static>,
        Bytes,
        BytesN<32>,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let selector = BytesN::from_array(&env, &[0x73, 0xc4, 0x57, 0xba]);
        let verifier_id = env.register(RiscZeroMockVerifier, (selector,));
        let verifier = RiscZeroMockVerifierClient::new(&env, &verifier_id);
        let owner = Address::generate(&env);
        let image_id = BytesN::from_array(&env, &IMAGE_ID);
        let contract_id = env.register(
            NeuroChainZkGuardrail,
            (
                owner,
                verifier_id.clone(),
                image_id.clone(),
                BytesN::from_array(&env, &policy_commitment),
                policy_version,
            ),
        );
        let client = NeuroChainZkGuardrailClient::new(&env, &contract_id);
        let journal_bytes = Bytes::from_slice(&env, &journal.encode().unwrap());
        (env, client, verifier, journal_bytes, image_id)
    }

    #[test]
    fn read_only_verify_does_not_consume_nullifier() {
        let public_journal = journal(
            DecisionStatus::Approved,
            ExitCode::Passed,
            ReasonCode::Passed,
            false,
            [0x43; 32],
        );
        let (env, client, verifier, journal_bytes, image_id) = setup(public_journal);
        let seal = mock_seal(&env, &verifier, &journal_bytes, &image_id);
        let accepted = client.verify(&seal, &journal_bytes);

        assert_eq!(accepted.exit_code, 0);
        assert_eq!(
            accepted.action_plan_hash,
            BytesN::from_array(&env, &[0x22; 32])
        );
        assert_eq!(
            accepted.policy_commitment,
            BytesN::from_array(&env, &[0x33; 32])
        );
        assert_eq!(accepted.policy_version, 1);
        assert!(!client.is_consumed(&accepted.audit_nullifier));
    }

    #[test]
    fn unauthorized_policy_commitment_fails_closed() {
        let public_journal = journal(
            DecisionStatus::Approved,
            ExitCode::Passed,
            ReasonCode::Passed,
            false,
            [0x46; 32],
        );
        let (env, client, verifier, journal_bytes, image_id) =
            setup_with_policy(public_journal, [0x99; 32], 1);
        let seal = mock_seal(&env, &verifier, &journal_bytes, &image_id);

        let Err(Ok(AttestationError::InvalidAttestation)) =
            client.try_verify(&seal, &journal_bytes)
        else {
            panic!("unauthorized policy must fail closed");
        };
        assert!(!client.is_consumed(&BytesN::from_array(&env, &[0x46; 32])));
    }

    fn mock_seal(
        env: &Env,
        verifier: &RiscZeroMockVerifierClient<'_>,
        journal_bytes: &Bytes,
        image_id: &BytesN<32>,
    ) -> Bytes {
        let digest: Hash<32> = env.crypto().sha256(journal_bytes);
        verifier.mock_prove(image_id, &digest.into()).seal
    }

    #[test]
    fn approved_attestation_is_consumed_once_and_replay_rejects() {
        let public_journal = journal(
            DecisionStatus::Approved,
            ExitCode::Passed,
            ReasonCode::Passed,
            false,
            [0x44; 32],
        );
        let (env, client, verifier, journal_bytes, image_id) = setup(public_journal);
        let seal = mock_seal(&env, &verifier, &journal_bytes, &image_id);
        let accepted = client.verify_and_consume(&seal, &journal_bytes);

        assert_eq!(accepted.exit_code, 0);
        assert_eq!(
            accepted.next_step,
            NextStep::EligibleForSeparateApprovalFlow
        );
        assert!(client.is_consumed(&accepted.audit_nullifier));

        let Err(Ok(AttestationError::Replay)) =
            client.try_verify_and_consume(&seal, &journal_bytes)
        else {
            panic!("expected replay rejection");
        };

        let mut invalid_seal = seal.clone();
        invalid_seal.set(4, invalid_seal.get(4).unwrap() ^ 1);
        let Err(Ok(AttestationError::InvalidAttestation)) =
            client.try_verify_and_consume(&invalid_seal, &journal_bytes)
        else {
            panic!("invalid proof must not be reported as replay");
        };
    }

    #[test]
    fn verifier_failure_does_not_consume_nullifier() {
        let public_journal = journal(
            DecisionStatus::Approved,
            ExitCode::Passed,
            ReasonCode::Passed,
            false,
            [0x45; 32],
        );
        let (env, client, verifier, journal_bytes, image_id) = setup(public_journal);
        let mut seal = mock_seal(&env, &verifier, &journal_bytes, &image_id);
        seal.set(4, seal.get(4).unwrap() ^ 1);
        let nullifier = BytesN::from_array(&env, &[0x45; 32]);

        let Err(Ok(AttestationError::InvalidAttestation)) =
            client.try_verify_and_consume(&seal, &journal_bytes)
        else {
            panic!("expected verifier rejection");
        };
        assert!(!client.is_consumed(&nullifier));
    }

    #[test]
    fn requires_approval_and_blocked_decisions_remain_non_submit() {
        let cases = [
            (
                DecisionStatus::RequiresApproval,
                ExitCode::Passed,
                ReasonCode::ApprovalThreshold,
                true,
                NextStep::RequiresApproval,
            ),
            (
                DecisionStatus::Blocked,
                ExitCode::Allowlist,
                ReasonCode::Allowlist,
                false,
                NextStep::Blocked,
            ),
            (
                DecisionStatus::Blocked,
                ExitCode::ContractPolicy,
                ReasonCode::ContractPolicy,
                false,
                NextStep::Blocked,
            ),
            (
                DecisionStatus::Blocked,
                ExitCode::IntentSafety,
                ReasonCode::IntentSafety,
                false,
                NextStep::Blocked,
            ),
        ];

        for (index, (decision, exit, reason, approval, expected)) in cases.into_iter().enumerate() {
            let mut nullifier = [0x50; 32];
            nullifier[0] = index as u8;
            let public_journal = journal(decision, exit, reason, approval, nullifier);
            let (env, client, verifier, journal_bytes, image_id) = setup(public_journal);
            let seal = mock_seal(&env, &verifier, &journal_bytes, &image_id);
            let accepted = client.verify_and_consume(&seal, &journal_bytes);

            assert_eq!(accepted.exit_code, exit as u32);
            assert_eq!(accepted.next_step, expected);
        }
    }
}
