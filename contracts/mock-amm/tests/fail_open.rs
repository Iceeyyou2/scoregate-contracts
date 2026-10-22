use mock_amm::{FailPolicy, MockAmm, MockAmmClient};
use scoregate_score::{ScoreGate, ScoreGateClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

fn setup<'a>() -> (Env, MockAmmClient<'a>, ScoreGateClient<'a>, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let amm_id = env.register_contract(None, MockAmm);
    let oracle_id = env.register_contract(None, ScoreGate);

    let amm = MockAmmClient::new(&env, &amm_id);
    let oracle = ScoreGateClient::new(&env, &oracle_id);

    let user = Address::generate(&env);

    (env, amm, oracle, user)
}

#[test]
fn swap_succeeds_under_fail_open_when_oracle_uninitialized() {
    let (_env, amm, _oracle, user) = setup();

    amm.set_fail_policy(&FailPolicy::FailOpen);

    // Oracle is left uninitialized: any score lookup traps.
    let result = amm.try_swap(&user, &1_000_i128);
    assert!(result.is_ok(), "swap must succeed under FailOpen when oracle traps");
}

#[test]
fn swap_rejected_under_fail_closed_when_oracle_uninitialized() {
    let (_env, amm, _oracle, user) = setup();

    amm.set_fail_policy(&FailPolicy::FailClosed);

    let result = amm.try_swap(&user, &1_000_i128);
    assert!(result.is_err(), "swap must be rejected under FailClosed when oracle traps");
}
