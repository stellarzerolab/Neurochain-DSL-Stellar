use groth16_verifier::RiscZeroGroth16Verifier;
use neurochain_zk_guardrail_contract::{DecisionStatus, ExitCode, PublicJournal};
use neurochain_zk_guardrail_soroban::{
    NeuroChainZkGuardrail, NeuroChainZkGuardrailClient, NextStep,
};
use risc0_router::{RiscZeroVerifierRouter, RiscZeroVerifierRouterClient};
use serde::Deserialize;
use soroban_sdk::{testutils::Address as _, Address, Bytes, BytesN, Env};

#[derive(Deserialize)]
struct Groth16Fixture {
    schema_version: u32,
    seal_hex: String,
    image_id_hex: String,
    journal_hex: String,
    journal_digest_hex: String,
}

fn decode_hex(value: &str) -> Vec<u8> {
    hex::decode(value).expect("fixture fields must contain valid hex")
}

fn decode_digest(value: &str) -> [u8; 32] {
    decode_hex(value)
        .try_into()
        .expect("fixture digest fields must contain 32 bytes")
}

#[test]
fn genuine_groth16_proof_verifies_and_consumes_nullifier() {
    let fixture: Groth16Fixture =
        serde_json::from_str(include_str!("../../fixtures/groth16_approved.json"))
            .expect("Groth16 fixture must be valid JSON");
    assert_eq!(fixture.schema_version, 1);

    let env = Env::default();
    let seal = Bytes::from_slice(&env, &decode_hex(&fixture.seal_hex));
    let journal_bytes = Bytes::from_slice(&env, &decode_hex(&fixture.journal_hex));
    let image_id = BytesN::from_array(&env, &decode_digest(&fixture.image_id_hex));
    let expected_journal_digest = decode_digest(&fixture.journal_digest_hex);
    let actual_journal_digest: BytesN<32> = env.crypto().sha256(&journal_bytes).into();
    assert_eq!(actual_journal_digest.to_array(), expected_journal_digest);

    let verifier_id = env.register(RiscZeroGroth16Verifier, ());
    let contract_id = env.register(NeuroChainZkGuardrail, (verifier_id, image_id.clone()));
    let client = NeuroChainZkGuardrailClient::new(&env, &contract_id);

    let accepted = client.verify_and_consume(&seal, &journal_bytes);
    assert_eq!(accepted.decision_status, DecisionStatus::Approved as u32);
    assert_eq!(accepted.exit_code, ExitCode::Passed as u32);
    assert!(!accepted.requires_approval);
    assert_eq!(
        accepted.next_step,
        NextStep::EligibleForSeparateApprovalFlow
    );
    assert!(client.is_consumed(&accepted.audit_nullifier));

    let journal = PublicJournal::decode(&decode_hex(&fixture.journal_hex))
        .expect("fixture must contain a canonical NeuroChain journal");
    assert_eq!(journal.evaluator_image_id, image_id.to_array());
    assert_eq!(journal.audit_nullifier, accepted.audit_nullifier.to_array());
}

#[test]
fn genuine_groth16_proof_routes_by_selector_and_consumes_nullifier() {
    let fixture: Groth16Fixture =
        serde_json::from_str(include_str!("../../fixtures/groth16_approved.json"))
            .expect("Groth16 fixture must be valid JSON");
    let env = Env::default();
    env.mock_all_auths();

    let seal_raw = decode_hex(&fixture.seal_hex);
    let seal = Bytes::from_slice(&env, &seal_raw);
    let journal_bytes = Bytes::from_slice(&env, &decode_hex(&fixture.journal_hex));
    let image_id = BytesN::from_array(&env, &decode_digest(&fixture.image_id_hex));
    let selector = BytesN::from_array(
        &env,
        &seal_raw[..4]
            .try_into()
            .expect("Groth16 seal must contain a four-byte selector"),
    );

    let verifier_id = env.register(RiscZeroGroth16Verifier, ());
    let admin = Address::generate(&env);
    let router_id = env.register(RiscZeroVerifierRouter, (admin,));
    let router = RiscZeroVerifierRouterClient::new(&env, &router_id);
    router.add_verifier(&selector, &verifier_id);
    assert_eq!(router.get_verifier_by_selector(&selector), verifier_id);

    let contract_id = env.register(NeuroChainZkGuardrail, (router_id, image_id.clone()));
    let client = NeuroChainZkGuardrailClient::new(&env, &contract_id);
    let accepted = client.verify_and_consume(&seal, &journal_bytes);

    assert_eq!(accepted.decision_status, DecisionStatus::Approved as u32);
    assert_eq!(accepted.exit_code, ExitCode::Passed as u32);
    assert_eq!(
        accepted.next_step,
        NextStep::EligibleForSeparateApprovalFlow
    );
    assert!(client.is_consumed(&accepted.audit_nullifier));
}
