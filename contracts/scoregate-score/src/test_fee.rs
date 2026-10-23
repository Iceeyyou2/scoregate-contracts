#![cfg(test)]

use soroban_sdk::{
    symbol_short,
    testutils::Address as _,
    token::{StellarAssetClient, TokenClient},
    Address, Env, Vec,
};

use crate::{
    Error, ScoreGateScoreContract, ScoreGateScoreContractClient,
};

fn setup<'a>() -> (Env, ScoreGateScoreContractClient<'a>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    client.initialize(&admin, &service);

    (env, client, admin, service)
}

/// Returns (env, client, admin, token_address, contract_id) with a mock
/// SEP-41 token funded to the contract with `initial_balance`.
fn setup_with_token<'a>(
    initial_balance: i128,
) -> (
    Env,
    ScoreGateScoreContractClient<'a>,
    Address,
    Address,
    Address,
) {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    client.initialize(&admin, &service);

    let issuer = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(issuer);
    let token_address = sac.address();

    if initial_balance > 0 {
        StellarAssetClient::new(&env, &token_address).mint(&contract_id, &initial_balance);
    }

    (env, client, admin, token_address, contract_id)
}

#[test]
fn test_set_fee_token_insufficient_signers_rejected() {
    let (env, client, _admin, _service) = setup();
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let s3 = Address::generate(&env);

    client.add_admin_signer(&Vec::new(&env), &s1);
    client.add_admin_signer(&Vec::new(&env), &s2);
    client.add_admin_signer(&Vec::new(&env), &s3);
    client.set_admin_threshold(&Vec::new(&env), &2);

    let fee_token = Address::generate(&env);

    // 0 signers when threshold is 2 -> InsufficientAdminSigners
    let res = client.try_set_fee_token(&Vec::new(&env), &fee_token);
    assert_eq!(res, Err(Ok(Error::InsufficientAdminSigners)));

    // 1 signer when threshold is 2 -> InsufficientAdminSigners
    let mut signers_1 = Vec::new(&env);
    signers_1.push_back(s1.clone());
    let res = client.try_set_fee_token(&signers_1, &fee_token);
    assert_eq!(res, Err(Ok(Error::InsufficientAdminSigners)));

    // Fee token was not set
    assert_eq!(client.get_fee_token(), None);
}

#[test]
fn test_set_fee_token_multisig_quorum_succeeds() {
    let (env, client, _admin, _service) = setup();
    let s1 = Address::generate(&env);
    let s2 = Address::generate(&env);
    let s3 = Address::generate(&env);

    client.add_admin_signer(&Vec::new(&env), &s1);
    client.add_admin_signer(&Vec::new(&env), &s2);
    client.add_admin_signer(&Vec::new(&env), &s3);
    client.set_admin_threshold(&Vec::new(&env), &2);

    let fee_token = Address::generate(&env);

    let mut signers_2 = Vec::new(&env);
    signers_2.push_back(s1);
    signers_2.push_back(s2);

    let res = client.try_set_fee_token(&signers_2, &fee_token);
    assert!(res.is_ok());
    assert_eq!(client.get_fee_token(), Some(fee_token));
}

#[test]
fn test_set_fee_token_legacy_single_admin_mode() {
    let (env, client, _admin, _service) = setup();
    let fee_token = Address::generate(&env);

    // In legacy single-admin mode (threshold = 0, no signers configured),
    // require_admin_auth requires legacy admin authorization.
    let res = client.try_set_fee_token(&Vec::new(&env), &fee_token);
    assert!(res.is_ok());
    assert_eq!(client.get_fee_token(), Some(fee_token));
}

#[test]
fn test_set_fee_token_signer_not_in_set_rejected() {
    let (env, client, _admin, _service) = setup();
    let s1 = Address::generate(&env);
    let rogue = Address::generate(&env);

    client.add_admin_signer(&Vec::new(&env), &s1);
    client.set_admin_threshold(&Vec::new(&env), &1);

    let fee_token = Address::generate(&env);
    let mut signers = Vec::new(&env);
    signers.push_back(rogue);

    let res = client.try_set_fee_token(&signers, &fee_token);
    assert_eq!(res, Err(Ok(Error::AdminSignerNotInSet)));
    assert_eq!(client.get_fee_token(), None);
}

#[test]
fn test_set_fee_token_uninitialized_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(&env, &contract_id);
    let fee_token = Address::generate(&env);

    let res = client.try_set_fee_token(&Vec::new(&env), &fee_token);
    assert_eq!(res, Err(Ok(Error::NotInitialized)));
}


// ── Issue #228: set_gate_query_fee negative guard and audit event ──────────

#[test]
fn test_set_gate_query_fee_negative_rejected() {
    let (_env, client, _admin, _service) = setup();
    let result = client.try_set_gate_query_fee(&(-100));
    assert_eq!(result, Err(Ok(Error::InvalidScore)));
}

#[test]
fn test_set_gate_query_fee_zero_allowed() {
    let (_env, client, _admin, _service) = setup();
    // Zero fee is valid — disables fee collection.
    let result = client.try_set_gate_query_fee(&0);
    assert!(result.is_ok());
}

#[test]
fn test_set_gate_query_fee_positive_succeeds() {
    let (_env, client, _admin, _service) = setup();
    let result = client.try_set_gate_query_fee(&500);
    assert!(result.is_ok());
}

#[test]
fn test_set_gate_query_fee_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(&env, &contract_id);
    let result = client.try_set_gate_query_fee(&100);
    assert_eq!(result, Err(Ok(Error::NotInitialized)));
}

// ── Issue #227: withdraw_fees validation and accounting ─────────────────────

#[test]
fn test_withdraw_fees_negative_amount_rejected() {
    let (env, client, _admin, token_address, _contract_id) = setup_with_token(100_000);
    client.set_fee_token(&Vec::new(&env), &token_address);
    let recipient = Address::generate(&env);
    client.set_fee_recipient(&Vec::new(&env), &recipient);
    let result = client.try_withdraw_fees(&Vec::new(&env), &recipient, &(-100));
    assert_eq!(result, Err(Ok(Error::InvalidWithdrawalAmount)));
}

#[test]
fn test_withdraw_fees_zero_amount_rejected() {
    let (env, client, _admin, token_address, _contract_id) = setup_with_token(100_000);
    client.set_fee_token(&Vec::new(&env), &token_address);
    let recipient = Address::generate(&env);
    client.set_fee_recipient(&Vec::new(&env), &recipient);
    let result = client.try_withdraw_fees(&Vec::new(&env), &recipient, &0);
    assert_eq!(result, Err(Ok(Error::InvalidWithdrawalAmount)));
}

#[test]
fn test_withdraw_fees_exceeds_accumulated_rejected() {
    let (env, client, _admin, token_address, contract_id) = setup_with_token(100_000);
    client.set_fee_token(&Vec::new(&env), &token_address);
    let recipient = Address::generate(&env);
    client.set_fee_recipient(&Vec::new(&env), &recipient);

    // Simulate accumulated fees of 1_000 by writing storage directly.
    env.as_contract(&contract_id, || {
        crate::storage::set_accumulated_fees(&env, 1_000);
    });

    // Withdraw 2_000 when only 1_000 accumulated → rejected.
    let result = client.try_withdraw_fees(&Vec::new(&env), &recipient, &2_000);
    assert_eq!(result, Err(Ok(Error::InsufficientFunds)));
}

#[test]
fn test_withdraw_fees_decrements_accumulated() {
    let (env, client, _admin, token_address, contract_id) = setup_with_token(100_000);
    client.set_fee_token(&Vec::new(&env), &token_address);
    let recipient = Address::generate(&env);
    client.set_fee_recipient(&Vec::new(&env), &recipient);

    // Set accumulated fees to 5_000.
    env.as_contract(&contract_id, || {
        crate::storage::set_accumulated_fees(&env, 5_000);
    });

    // Withdraw 3_000 — should succeed and decrement counter.
    client.withdraw_fees(&Vec::new(&env), &recipient, &3_000);

    // Accumulated fees should be 2_000 after withdrawal.
    let remaining = env.as_contract(&contract_id, || {
        crate::storage::get_accumulated_fees(&env)
    });
    assert_eq!(remaining, 2_000);
}

#[test]
fn test_withdraw_fees_full_accumulated_succeeds() {
    let (env, client, _admin, token_address, contract_id) = setup_with_token(100_000);
    client.set_fee_token(&Vec::new(&env), &token_address);
    let recipient = Address::generate(&env);
    client.set_fee_recipient(&Vec::new(&env), &recipient);

    // Set accumulated fees to 50_000.
    env.as_contract(&contract_id, || {
        crate::storage::set_accumulated_fees(&env, 50_000);
    });

    // Withdraw the full amount.
    client.withdraw_fees(&Vec::new(&env), &recipient, &50_000);

    // Accumulated should be 0.
    let remaining = env.as_contract(&contract_id, || {
        crate::storage::get_accumulated_fees(&env)
    });
    assert_eq!(remaining, 0);
}

// ── Issue #229: query_risk_gate_paid fee charging ────────────────────────────

#[test]
fn test_query_risk_gate_pure_no_fee() {
    // query_risk_gate remains pure read — no fee, no accumulation.
    let (env, client, _admin, _service) = setup();
    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");
    // No score → false.
    assert!(!client.query_risk_gate(&wallet, &pair, &75));
    assert_eq!(client.get_accumulated_fees(), 0);
}

#[test]
fn test_query_risk_gate_paid_zero_fee_no_accumulation() {
    let (env, client, _admin, token_address, _contract_id) = setup_with_token(0);
    client.set_fee_token(&Vec::new(&env), &token_address);
    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");
    // Default fee = 0: paid gate call works, no accumulation.
    client.query_risk_gate_paid(&wallet, &wallet, &pair, &75);
    assert_eq!(client.get_accumulated_fees(), 0);
}

#[test]
fn test_query_risk_gate_paid_charges_and_accumulates() {
    let (env, client, _admin, token_address, contract_id) = setup_with_token(0);
    client.set_fee_token(&Vec::new(&env), &token_address);

    let fee: i128 = 200;
    client.set_gate_query_fee(&fee).unwrap();

    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");
    let caller = Address::generate(&env);

    // Fund the caller with tokens.
    StellarAssetClient::new(&env, &token_address).mint(&caller, &10_000);

    // Submit a score so the gate returns true.
    client
        .submit_score(
            &Vec::new(&env),
            &wallet,
            &pair,
            &30,
            &false,
            &false,
            &1,
            &90,
            &1,
            &None,
        )
        .unwrap();

    let result = client.query_risk_gate_paid(&caller, &wallet, &pair, &75);
    assert!(result);
    assert_eq!(client.get_accumulated_fees(), fee);

    // Verify the contract received the fee.
    let token = TokenClient::new(&env, &token_address);
    assert_eq!(token.balance(&contract_id), fee);
    assert_eq!(token.balance(&caller), 10_000 - fee);
}

#[test]
fn test_query_risk_gate_paid_accumulates_across_calls() {
    let (env, client, _admin, token_address, contract_id) = setup_with_token(0);
    client.set_fee_token(&Vec::new(&env), &token_address);

    let fee: i128 = 100;
    client.set_gate_query_fee(&fee).unwrap();

    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");
    let caller = Address::generate(&env);

    StellarAssetClient::new(&env, &token_address).mint(&caller, &10_000);

    client
        .submit_score(
            &Vec::new(&env),
            &wallet,
            &pair,
            &30,
            &false,
            &false,
            &1,
            &90,
            &1,
            &None,
        )
        .unwrap();

    client.query_risk_gate_paid(&caller, &wallet, &pair, &75);
    client.query_risk_gate_paid(&caller, &wallet, &pair, &75);
    client.query_risk_gate_paid(&caller, &wallet, &pair, &75);

    assert_eq!(client.get_accumulated_fees(), fee * 3);
    let token = TokenClient::new(&env, &token_address);
    assert_eq!(token.balance(&contract_id), fee * 3);
}

#[test]
fn test_query_risk_gate_paid_no_token_no_accumulation() {
    let (env, client, _admin, _token_address, _contract_id) = setup();
    // No fee token configured, but fee is set.
    client.set_gate_query_fee(&999).unwrap();

    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");
    let caller = Address::generate(&env);

    // Should not panic, no accumulation.
    let result = client.query_risk_gate_paid(&caller, &wallet, &pair, &75);
    assert!(!result); // no score → false
    assert_eq!(client.get_accumulated_fees(), 0);
}

#[test]
fn test_query_risk_gate_paid_then_withdraw_decrements() {
    let (env, client, _admin, token_address, contract_id) = setup_with_token(0);
    client.set_fee_token(&Vec::new(&env), &token_address);

    let fee: i128 = 500;
    client.set_gate_query_fee(&fee).unwrap();

    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");
    let caller = Address::generate(&env);
    StellarAssetClient::new(&env, &token_address).mint(&caller, &10_000);

    client
        .submit_score(
            &Vec::new(&env),
            &wallet,
            &pair,
            &30,
            &false,
            &false,
            &1,
            &90,
            &1,
            &None,
        )
        .unwrap();

    // Two paid queries → 1_000 accumulated.
    client.query_risk_gate_paid(&caller, &wallet, &pair, &75);
    client.query_risk_gate_paid(&caller, &wallet, &pair, &75);
    assert_eq!(client.get_accumulated_fees(), fee * 2);

    // Set up recipient and withdraw.
    let recipient = Address::generate(&env);
    client.set_fee_recipient(&Vec::new(&env), &recipient);
    client.withdraw_fees(&Vec::new(&env), &recipient, &fee);

    // Accumulated should be decremented.
    assert_eq!(client.get_accumulated_fees(), fee);

    // Verify token balances.
    let token = TokenClient::new(&env, &token_address);
    assert_eq!(token.balance(&recipient), fee);
    assert_eq!(token.balance(&contract_id), fee); // remaining accumulated
}
