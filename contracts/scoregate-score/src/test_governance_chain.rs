//! Tests for #299: Merkle governance audit chain.

use soroban_sdk::{
    testutils::Address as _,
    Address, BytesN, Env, Vec,
};

use crate::{ScoreGateScoreContract, ScoreGateScoreContractClient};

fn setup<'a>() -> (Env, ScoreGateScoreContractClient<'a>) {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    client.initialize(&admin, &service);
    (env, client)
}

// ── genesis head is all-zeros ─────────────────────────────────────────────────

#[test]
fn test_initial_governance_chain_head_is_zero() {
    let (env, client) = setup();
    let head = client.get_governance_chain_head();
    assert_eq!(head, BytesN::from_array(&env, &[0u8; 32]));
}

// ── head advances after an admin action ──────────────────────────────────────

#[test]
fn test_head_advances_on_admin_action() {
    let (env, client) = setup();
    let head_before = client.get_governance_chain_head();

    // set_gate_enforcement_mode is an admin action that appends to the chain.
    client.set_gate_enforcement_mode(&Vec::new(&env), &true);

    let head_after = client.get_governance_chain_head();
    assert_ne!(head_before, head_after, "head must change after admin action");
}

// ── chain is monotonic: each action produces a different head ─────────────────

#[test]
fn test_chain_is_monotonic() {
    let (env, client) = setup();

    client.set_gate_enforcement_mode(&Vec::new(&env), &true);
    let h1 = client.get_governance_chain_head();

    client.set_gate_enforcement_mode(&Vec::new(&env), &false);
    let h2 = client.get_governance_chain_head();

    assert_ne!(h1, h2);
}

// ── verify_governance_action with empty proof verifies a single-step chain ───

#[test]
fn test_verify_governance_action_single_step() {
    let (env, client) = setup();
    // After one action the new head = SHA256(0x01 || zeros32 || action_data).
    // We reproduce the action_data for set_gate_enforcement_mode(true): [0u8; 31, 1u8].
    let mut action_data = [0u8; 32];
    action_data[31] = 1u8; // strict = true

    // Derive action_hash = SHA256(0x01 || genesis(zeros) || action_data)
    let genesis = BytesN::<32>::from_array(&env, &[0u8; 32]);
    let mut buf = [0u8; 65];
    buf[0] = 0x01;
    buf[1..33].copy_from_slice(&genesis.to_array());
    buf[33..65].copy_from_slice(&action_data);
    let action_hash = env.crypto().sha256(&soroban_sdk::Bytes::from_array(&env, &buf));
    let action_hash_bytes = BytesN::<32>::from_array(&env, &action_hash.to_bytes().to_array());

    // Trigger the action.
    client.set_gate_enforcement_mode(&Vec::new(&env), &true);

    // With an empty proof, action_hash itself should equal the chain head.
    let result = client.verify_governance_action(&action_hash_bytes, &Vec::new(&env));
    assert!(result, "single-step chain verification must succeed");
}

// ── verify_governance_action with wrong hash returns false ───────────────────

#[test]
fn test_verify_governance_action_wrong_hash_fails() {
    let (env, client) = setup();
    client.set_gate_enforcement_mode(&Vec::new(&env), &true);

    let bad_hash = BytesN::from_array(&env, &[0xffu8; 32]);
    assert!(!client.verify_governance_action(&bad_hash, &Vec::new(&env)));
}

// ── #285: audit root preimage is unified and reconstructable offline ─────────

#[test]
fn test_audit_root_reconstructable_offline() {
    let (env, client) = setup();

    // Two sequential admin actions. Each must advance the audit root using the
    // same preimage format as append_governance_action_raw:
    //   new_root = SHA256(0x01 || old_root || action_data_32)
    // so an off-chain replay tool can deterministically rebuild the root from
    // the recorded action payloads alone (no timestamp/sequence divergence).
    let genesis = BytesN::<32>::from_array(&env, &[0u8; 32]);

    let mut action_data_1 = [0u8; 32];
    action_data_1[31] = 1u8; // strict = true

    let mut action_data_2 = [0u8; 32];
    action_data_2[31] = 0u8; // strict = false

    // Offline reconstruction of the expected head after both actions.
    let expected_1 = sha256_chain_step(&env, &genesis, &action_data_1);
    let expected_2 = sha256_chain_step(&env, &expected_1, &action_data_2);

    // On-chain execution of the same two actions.
    client.set_gate_enforcement_mode(&Vec::new(&env), &true);
    let head_1 = client.get_governance_chain_head();
    client.set_gate_enforcement_mode(&Vec::new(&env), &false);
    let head_2 = client.get_governance_chain_head();

    assert_eq!(
        head_1, expected_1,
        "audit root after first action must match offline reconstruction"
    );
    assert_eq!(
        head_2, expected_2,
        "audit root after second action must match offline reconstruction"
    );
}

fn sha256_chain_step(env: &Env, old_root: &BytesN<32>, action_data: &[u8; 32]) -> BytesN<32> {
    let mut buf = [0u8; 65];
    buf[0] = 0x01;
    buf[1..33].copy_from_slice(&old_root.to_array());
    buf[33..65].copy_from_slice(action_data);
    let digest = env.crypto().sha256(&soroban_sdk::Bytes::from_array(env, &buf));
    BytesN::<32>::from_array(env, &digest.to_bytes().to_array())
}

// ── #278: remove_service_signer appends to the audit chain ────────────────────

#[test]
fn test_remove_service_signer_appends_to_audit_chain() {
    let (env, client) = setup();

    // Add a service signer first (this already appends to the chain).
    let signer = Address::generate(&env);
    client.add_service_signer(&Vec::new(&env), &signer);
    let head_after_add = client.get_governance_chain_head();

    // Remove the signer — this must also append to the chain.
    client.remove_service_signer(&Vec::new(&env), &signer);
    let head_after_remove = client.get_governance_chain_head();

    assert_ne!(
        head_after_add, head_after_remove,
        "audit chain head must advance after remove_service_signer"
    );
}
