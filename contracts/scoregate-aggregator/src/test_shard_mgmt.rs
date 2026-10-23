//! Unit tests for shard management operations, specifically for the fix to
//! issue #238: `remove_shard` must clear `LastShardFailure` if it references
//! the shard being removed.

use crate::{Error, ScoreGateAggregator, ScoreGateAggregatorClient};
use scoregate_score::{ScoreGateScoreContract, ScoreGateScoreContractClient};
use scoregate_test_support::{generate_score_roles, test_env};
use soroban_sdk::{symbol_short, testutils::Address as _, Address, Env, Vec};

/// Registers and initializes a real score shard.
fn setup_score_shard<'a>(env: &Env) -> (Address, ScoreGateScoreContractClient<'a>) {
    let id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(env, &id);
    let (admin, service) = generate_score_roles(env);
    client.initialize(&admin, &service);
    (id, client)
}

/// Initializes an aggregator with the given admin.
fn init_aggregator(env: &Env, admin: &Address) -> ScoreGateAggregatorClient<'_> {
    let agg_id = env.register_contract(None, ScoreGateAggregator);
    let client = ScoreGateAggregatorClient::new(env, &agg_id);
    client.initialize(admin);
    client
}

/// Triggers a transport-level failure on the given shard by making it return
/// an error from `get_score`. Since we can't easily simulate a host trap in the
/// test environment, we instead manually inject a `LastShardFailure` record
/// to test the cleanup logic.
fn inject_last_shard_failure(
    env: &Env,
    client: &ScoreGateAggregatorClient,
    shard: &Address,
    failure_code: u32,
) {
    // Directly write to storage using the contract's DataKey
    // We need to use env.as_contract to write to the aggregator's instance storage
    env.as_contract(&client.contract_id(), || {
        env.storage()
            .instance()
            .set(&crate::DataKey::LastShardFailure, &(shard.clone(), failure_code));
    });
}

#[test]
fn test_remove_shard_clears_last_shard_failure_when_matches() {
    let env = test_env();
    let admin = Address::generate(&env);
    let client = init_aggregator(&env, &admin);

    // Add a shard
    let (shard_id, _shard_client) = setup_score_shard(&env);
    client.add_shard(&shard_id);

    // Inject a LastShardFailure record for this shard (simulating a prior transport failure)
    inject_last_shard_failure(&env, &client, &shard_id, 0); // FAILURE_TRANSPORT = 0

    // Verify the failure record exists
    let failure = client.get_last_shard_failure();
    assert!(failure.is_some(), "LastShardFailure should be set before removal");
    let (failed_shard, code) = failure.unwrap();
    assert_eq!(failed_shard, shard_id);
    assert_eq!(code, 0);

    // Remove the shard
    client.remove_shard(&shard_id);

    // Verify the LastShardFailure has been cleared
    let failure_after_removal = client.get_last_shard_failure();
    assert_eq!(
        failure_after_removal,
        None,
        "LastShardFailure must be cleared when the shard is removed"
    );
}

#[test]
fn test_remove_shard_preserves_last_shard_failure_when_different_shard() {
    let env = test_env();
    let admin = Address::generate(&env);
    let client = init_aggregator(&env, &admin);

    // Add two shards
    let (shard1_id, _shard1_client) = setup_score_shard(&env);
    let (shard2_id, _shard2_client) = setup_score_shard(&env);
    client.add_shard(&shard1_id);
    client.add_shard(&shard2_id);

    // Inject a LastShardFailure record for shard1
    inject_last_shard_failure(&env, &client, &shard1_id, 1); // FAILURE_CONTRACT_ERROR = 1

    // Verify the failure record exists for shard1
    let failure = client.get_last_shard_failure();
    assert!(failure.is_some());
    let (failed_shard, code) = failure.unwrap();
    assert_eq!(failed_shard, shard1_id);
    assert_eq!(code, 1);

    // Remove shard2 (different from the one in LastShardFailure)
    client.remove_shard(&shard2_id);

    // Verify the LastShardFailure is still there (for shard1)
    let failure_after_removal = client.get_last_shard_failure();
    assert!(failure_after_removal.is_some(), "LastShardFailure for shard1 must remain");
    let (failed_shard, code) = failure_after_removal.unwrap();
    assert_eq!(failed_shard, shard1_id);
    assert_eq!(code, 1);
}

#[test]
fn test_remove_shard_no_last_shard_failure_set() {
    let env = test_env();
    let admin = Address::generate(&env);
    let client = init_aggregator(&env, &admin);

    // Add a shard
    let (shard_id, _shard_client) = setup_score_shard(&env);
    client.add_shard(&shard_id);

    // Verify no failure record exists initially
    assert_eq!(client.get_last_shard_failure(), None);

    // Remove the shard
    client.remove_shard(&shard_id);

    // Verify no failure record exists after removal
    assert_eq!(client.get_last_shard_failure(), None);
}

#[test]
fn test_get_last_shard_failure_after_shard_removal_returns_none() {
    let env = test_env();
    let admin = Address::generate(&env);
    let client = init_aggregator(&env, &admin);

    // Add a shard
    let (shard_id, _shard_client) = setup_score_shard(&env);
    client.add_shard(&shard_id);

    // Inject a LastShardFailure record
    inject_last_shard_failure(&env, &client, &shard_id, 0);

    // Remove the shard
    client.remove_shard(&shard_id);

    // Query get_last_shard_failure - should return None
    let result = client.get_last_shard_failure();
    assert_eq!(
        result,
        None,
        "get_last_shard_failure must return None after the failed shard is decommissioned"
    );
}