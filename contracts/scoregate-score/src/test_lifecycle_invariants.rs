//! Randomised lifecycle-churn tests for the score storage invariants.
//!
//! Executes random sequences of `submit_score`, `submit_scores_batch`,
//! `clear_score` and `clear_score_history_with_audit` over a small universe of
//! wallets and pairs (small, so collisions / re-creations / deletions are
//! frequent), running `invariant_check` after every step and asserting that
//! the derived indices exactly mirror the live records:
//!
//! - #7/#8  `ScoreEntryIndex` == set of live `Score` keys, no duplicates
//! - #9     `ScoreCount` > 0 iff a live `Score` exists
//! - #13    `AssetPairs(wallet)` only lists pairs with live data
//! - #15    histogram total == number of live scores
//!
//! The repo has no external proptest dependency (see
//! `test_aggregate_invariants.rs`), so inputs come from a seeded xorshift64
//! PRNG: every CI run replays the same sequences and a failing seed is
//! printed for reproduction.

#![cfg(test)]

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, Bytes, Env, Symbol, Vec,
};

use crate::{
    constants::DEFAULT_COOLDOWN_SECS, invariants, storage, types::ScoreSubmission,
    ScoreGateScoreContract, ScoreGateScoreContractClient,
};

const START_TS: u64 = 1_700_000_000;
const WALLETS: usize = 4;
const PAIRS: [&str; 3] = ["XLM_USDC", "BTC_USDC", "ETH_USDC"];
const SEQUENCES: u64 = 40;
const STEPS_PER_SEQUENCE: u32 = 30;

struct Xorshift64(u64);
impl Xorshift64 {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

struct Harness<'a> {
    env: Env,
    client: ScoreGateScoreContractClient<'a>,
    contract_id: Address,
    wallets: std::vec::Vec<Address>,
    pairs: std::vec::Vec<Symbol>,
}

fn harness<'a>() -> Harness<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.budget().reset_unlimited();
    env.ledger().with_mut(|l| l.timestamp = START_TS);
    let contract_id = env.register_contract(None, ScoreGateScoreContract);
    let client = ScoreGateScoreContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let service = Address::generate(&env);
    client.initialize(&admin, &service);
    let wallets = (0..WALLETS).map(|_| Address::generate(&env)).collect();
    let pairs = PAIRS.iter().map(|p| Symbol::new(&env, p)).collect();
    Harness { env, client, contract_id, wallets, pairs }
}

impl Harness<'_> {
    fn now(&self) -> u64 {
        self.env.ledger().timestamp()
    }

    /// Advance past the per-(wallet, pair) cooldown so submissions are accepted.
    fn tick(&self) {
        self.env.ledger().with_mut(|l| l.timestamp += DEFAULT_COOLDOWN_SECS + 1);
    }

    fn submit(&self, w: usize, p: usize, score: u32) {
        let _ = self.client.try_submit_score(
            &Vec::new(&self.env),
            &self.wallets[w],
            &self.pairs[p],
            &score,
            &false,
            &false,
            &self.now(),
            &90,
            &1,
            &None,
        );
    }

    fn submit_batch(&self, rng: &mut Xorshift64) {
        let n = 1 + rng.below(4);
        let mut subs = Vec::new(&self.env);
        for _ in 0..n {
            subs.push_back(ScoreSubmission {
                wallet: self.wallets[rng.below(WALLETS as u64) as usize].clone(),
                asset_pair: self.pairs[rng.below(PAIRS.len() as u64) as usize].clone(),
                score: rng.below(101) as u32,
                benford_flag: false,
                ml_flag: false,
                timestamp: self.now(),
                confidence: 90,
                model_version: 1,
            });
        }
        let _ = self.client.try_submit_scores_batch(&Vec::new(&self.env), &subs);
    }

    fn clear(&self, w: usize, p: usize) {
        let _ = self.client.try_clear_score(
            &Vec::new(&self.env),
            &self.wallets[w],
            &self.pairs[p],
        );
    }

    fn clear_history(&self, w: usize, p: usize) {
        let _ = self.client.try_clear_score_history_with_audit(
            &Vec::new(&self.env),
            &self.wallets[w],
            &self.pairs[p],
            &Bytes::from_slice(&self.env, b"lifecycle-test"),
            &Bytes::from_slice(&self.env, b"test"),
        );
    }

    /// Run the contract invariant checker, then cross-check every derived
    /// structure against the ground truth obtained by probing every
    /// (wallet, pair) in the universe.
    fn assert_consistent(&self, ctx: &str) {
        self.env.as_contract(&self.contract_id, || {
            invariants::invariant_check(&self.env);

            let index = storage::get_score_entry_index(&self.env);
            let mut live = 0u32;
            for w in &self.wallets {
                for p in &self.pairs {
                    let has_score = storage::peek_score(&self.env, w, p).is_some();
                    let in_index = index.iter().filter(|(iw, ip)| iw == w && ip == p).count();
                    if has_score {
                        live += 1;
                        assert_eq!(in_index, 1, "{ctx}: live score missing/duplicated in index");
                        assert!(
                            storage::get_score_count(&self.env, w, p) > 0,
                            "{ctx}: live score with zero ScoreCount"
                        );
                    } else {
                        assert_eq!(in_index, 0, "{ctx}: ghost ScoreEntryIndex key");
                        assert_eq!(
                            storage::get_score_count(&self.env, w, p),
                            0,
                            "{ctx}: ScoreCount without live score"
                        );
                    }
                    if storage::get_wallet_pairs(&self.env, w).contains(p) {
                        let history = storage::peek_score_history_len(&self.env, w, p);
                        assert!(
                            has_score || history > 0,
                            "{ctx}: ghost AssetPairs entry"
                        );
                    }
                }
            }
            assert_eq!(index.len(), live, "{ctx}: index length != live record count");

            let hist = storage::get_score_histogram(&self.env);
            let total: u64 = hist.buckets.iter().sum();
            assert_eq!(total, live as u64, "{ctx}: histogram total != live record count");
        });
    }
}

fn run_sequence(seed: u64) {
    let h = harness();
    let mut rng = Xorshift64(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    for step in 0..STEPS_PER_SEQUENCE {
        h.tick();
        let w = rng.below(WALLETS as u64) as usize;
        let p = rng.below(PAIRS.len() as u64) as usize;
        let op = rng.below(10);
        match op {
            0..=3 => h.submit(w, p, rng.below(101) as u32),
            4..=5 => h.submit_batch(&mut rng),
            6..=7 => h.clear(w, p),
            8 => h.clear_history(w, p),
            // clear -> resubmit on the same key (the lifecycle from the issue)
            _ => {
                h.clear(w, p);
                h.assert_consistent(&format!("seed={seed} step={step} after clear"));
                h.tick();
                h.submit(w, p, rng.below(101) as u32);
            }
        }
        h.assert_consistent(&format!("seed={seed} step={step} op={op}"));
    }
}

#[test]
fn lifecycle_churn_preserves_invariants() {
    // 40 sequences x 30 steps = 1200 checked steps.
    for seed in 1..=SEQUENCES {
        run_sequence(seed);
    }
}

#[test]
fn full_teardown_leaves_no_residue() {
    let h = harness();
    let mut rng = Xorshift64(0xC0FF_EE11);
    for _ in 0..40 {
        h.tick();
        h.submit(
            rng.below(WALLETS as u64) as usize,
            rng.below(PAIRS.len() as u64) as usize,
            rng.below(101) as u32,
        );
        h.assert_consistent("build-up");
    }
    for w in 0..WALLETS {
        for p in 0..PAIRS.len() {
            h.clear(w, p);
            h.clear_history(w, p);
            h.assert_consistent("teardown");
        }
    }
    h.env.as_contract(&h.contract_id, || {
        assert_eq!(storage::get_score_entry_index(&h.env).len(), 0, "ghost index keys remain");
        let total: u64 = storage::get_score_histogram(&h.env).buckets.iter().sum();
        assert_eq!(total, 0, "histogram not empty after teardown");
        for w in &h.wallets {
            assert!(storage::get_wallet_pairs(&h.env, w).is_empty(), "ghost AssetPairs entry");
        }
    });
}
