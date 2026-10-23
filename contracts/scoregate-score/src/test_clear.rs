//! Tests for issue #225: clear_score must remove JumpStats and BandEntryTime
//! auxiliary keys and decrement PairScoreCount / total_wallets_scored counters.

use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Vec};

use crate::{ScoreGateScoreContract, ScoreGateScoreContractClient};

const TS: u64 = 1_700_000_000;

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

fn submit(env: &Env, client: &ScoreGateScoreContractClient, wallet: &Address, score: u32) {
    client.submit_score(
        &Vec::new(env),
        wallet,
        &symbol_short!("XLM_USDC"),
        &score,
        &false,
        &false,
        &TS,
        &80u32,
        &1u32,
        &None,
    );
}

// ── Counter accuracy ──────────────────────────────────────────────────────────

/// Clearing the only scored wallet should bring both counters back to zero.
#[test]
fn test_clear_score_decrements_total_wallets_scored() {
    let (env, client, _admin, _service) = setup();
    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");

    submit(&env, &client, &wallet, 60);
    assert_eq!(client.get_total_wallets_scored(), 1);
    assert_eq!(client.get_pair_score_count(&pair), 1);

    client.clear_score(&Vec::new(&env), &wallet, &pair);

    assert_eq!(client.get_total_wallets_scored(), 0, "total_wallets_scored must be decremented");
    assert_eq!(client.get_pair_score_count(&pair), 0, "pair_score_count must be decremented");
}

/// Clearing one wallet out of two should leave counters at 1.
#[test]
fn test_clear_score_decrements_only_one_wallet() {
    let (env, client, _admin, _service) = setup();
    let wallet_a = Address::generate(&env);
    let wallet_b = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");

    submit(&env, &client, &wallet_a, 50);
    submit(&env, &client, &wallet_b, 70);
    assert_eq!(client.get_total_wallets_scored(), 2);
    assert_eq!(client.get_pair_score_count(&pair), 2);

    client.clear_score(&Vec::new(&env), &wallet_a, &pair);

    assert_eq!(client.get_total_wallets_scored(), 1);
    assert_eq!(client.get_pair_score_count(&pair), 1);
}

// ── Score lookup after clear ──────────────────────────────────────────────────

/// After clearing, get_score must return ScoreNotFound.
#[test]
fn test_clear_score_removes_primary_entry() {
    let (env, client, _admin, _service) = setup();
    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");

    submit(&env, &client, &wallet, 55);
    assert!(client.try_get_score(&wallet, &pair).is_ok());

    client.clear_score(&Vec::new(&env), &wallet, &pair);

    let result = client.try_get_score(&wallet, &pair);
    assert!(result.is_err(), "get_score should return ScoreNotFound after clear_score");
}

// ── JumpStats cleanup ─────────────────────────────────────────────────────────

/// After clearing, get_jump_stats must return (0, 0) — the key is gone.
#[test]
fn test_clear_score_removes_jump_stats() {
    let (env, client, _admin, _service) = setup();
    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");

    // First submission establishes a baseline.
    submit(&env, &client, &wallet, 30);
    // Second submission with a larger score triggers a jump record.
    client.submit_score(
        &Vec::new(&env),
        &wallet,
        &pair,
        &80,
        &false,
        &false,
        &(TS + 7200),
        &90u32,
        &1u32,
        &None,
    );
    // Confirm jump stats are non-zero.
    let (max_jump, _at_ts) = client.get_jump_stats(&wallet, &pair);
    assert!(max_jump > 0, "expected a recorded jump before clear");

    client.clear_score(&Vec::new(&env), &wallet, &pair);

    // After clear, jump stats must be zeroed (key removed → default (0,0)).
    let (max_jump_after, at_ts_after) = client.get_jump_stats(&wallet, &pair);
    assert_eq!(max_jump_after, 0, "JumpStats key must be removed by clear_score");
    assert_eq!(at_ts_after, 0, "JumpStats timestamp must be removed by clear_score");
}

// ── Idempotency ───────────────────────────────────────────────────────────────

/// Calling clear_score on a wallet with no score must be a no-op (no panic,
/// counters stay at 0).
#[test]
fn test_clear_score_is_noop_when_no_score_exists() {
    let (env, client, _admin, _service) = setup();
    let wallet = Address::generate(&env);
    let pair = symbol_short!("XLM_USDC");

    // No score submitted; clearing should not panic or underflow counters.
    client.clear_score(&Vec::new(&env), &wallet, &pair);

    assert_eq!(client.get_total_wallets_scored(), 0);
    assert_eq!(client.get_pair_score_count(&pair), 0);
}
