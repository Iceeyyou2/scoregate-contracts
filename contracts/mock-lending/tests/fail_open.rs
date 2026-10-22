use mock_lending::{FailPolicy, MockLending, MockLendingClient};
use scoregate_score::{ScoreGate, ScoreGateClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup(env: &Env) -> (MockLendingClient<'_>, ScoreGateClient<'_>, Address) {
    let admin = Address::generate(env);
    let oracle_id = env.register_contract(None, ScoreGate);
    let oracle = ScoreGateClient::new(env, &oracle_id);
    oracle.initialize(&admin);

    let lending_id = env.register_contract(None, MockLending);
    let lending = MockLendingClient::new(env, &lending_id);
    lending.initialize(&admin, &oracle_id);

    let user = Address::generate(env);
    (lending, oracle, user)
}

#[test]
fn borrow_succeeds_under_fail_open_when_oracle_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let (lending, oracle, user) = setup(&env);

    lending.set_fail_policy(&FailPolicy::FailOpen);

    // Force the oracle to trap on the next score lookup.
    oracle.set_trap(&true);

    // FailOpen must let the borrow proceed despite the oracle reverting.
    lending.borrow(&user, &1_000_i128);
}

#[test]
fn borrow_succeeds_under_fail_open_when_oracle_uninitialized() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    // Register an oracle but never initialize it, so calls revert.
    let oracle_id = env.register_contract(None, ScoreGate);

    let lending_id = env.register_contract(None, MockLending);
    let lending = MockLendingClient::new(&env, &lending_id);
    lending.initialize(&admin, &oracle_id);
    lending.set_fail_policy(&FailPolicy::FailOpen);

    let user = Address::generate(&env);
    lending.borrow(&user, &1_000_i128);
}

#[test]
#[should_panic]
fn borrow_rejected_under_fail_closed_when_oracle_reverts() {
    let env = Env::default();
    env.mock_all_auths();
    let (lending, oracle, user) = setup(&env);

    lending.set_fail_policy(&FailPolicy::FailClosed);
    oracle.set_trap(&true);

    lending.borrow(&user, &1_000_i128);
}

#[test]
#[should_panic]
fn borrow_rejected_under_fail_closed_when_oracle_uninitialized() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let oracle_id = env.register_contract(None, ScoreGate);

    let lending_id = env.register_contract(None, MockLending);
    let lending = MockLendingClient::new(&env, &lending_id);
    lending.initialize(&admin, &oracle_id);
    lending.set_fail_policy(&FailPolicy::FailClosed);

    let user = Address::generate(&env);
    lending.borrow(&user, &1_000_i128);
}
