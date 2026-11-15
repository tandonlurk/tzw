//! Property-style tests of the double-entry ledger under random posting
//! sequences (seeded xorshift, no extra dependencies).

use std::collections::BTreeMap;

use aen_core::ledger::*;
use aen_core::money::{Money, SignedMoney};
use uuid::Uuid;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

fn m(u: i64) -> Money {
    Money::from_minor(u).unwrap()
}
fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

type Balances = BTreeMap<Account, SignedMoney>;

fn apply(b: &mut Balances, set: &PostingSet) -> Result<(), LedgerError> {
    set.check_balances(b)?;
    for (a, d) in set.net_deltas() {
        let e = b.entry(a).or_default();
        *e = e.checked_add(d).expect("no overflow in test amounts");
    }
    Ok(())
}

fn total(b: &Balances) -> i128 {
    b.values().map(|v| i128::from(v.minor())).sum()
}

fn invariants(b: &Balances, deposited: i128) {
    assert_eq!(total(b), 0, "ledger must sum to zero");
    for (a, v) in b {
        if !a.may_go_negative() {
            assert!(!v.is_negative(), "{a} went negative: {v}");
        }
    }
    let ext = b
        .get(&Account::External)
        .map_or(0, |v| i128::from(v.minor()));
    assert_eq!(ext, -deposited, "external must mirror deposits exactly");
    let held: i128 = b
        .iter()
        .filter(|(a, _)| !a.may_go_negative())
        .map(|(_, v)| i128::from(v.minor()))
        .sum();
    assert_eq!(held, deposited, "funds are neither created nor destroyed");
}

#[test]
fn random_operation_sequences_preserve_invariants() {
    for seed in 1..=40u64 {
        let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let agents = [id(1), id(2), id(3)];
        let mut b = Balances::new();
        let mut deposited: i128 = 0;
        let mut ok = 0;
        let mut rejected = 0;
        for step in 0..600 {
            let key = format!("k{seed}-{step}");
            let amount = m(1 + rng.below(500) as i64);
            let a = agents[rng.below(3) as usize];
            let other = agents[rng.below(3) as usize];
            let ix = id(100 + rng.below(4) as u128);
            let r = match rng.below(6) {
                0 => deposit(Env::Simulated, amount, &key),
                1 => allocate(Env::Simulated, a, amount, &key),
                2 => deallocate(Env::Simulated, a, amount, &key),
                3 => commit_stake(Env::Simulated, a, ix, amount),
                4 => settle(Env::Simulated, ix, a, other, amount),
                _ => refund(Env::Simulated, a, ix, amount),
            };
            let Ok(set) = r else { continue };
            // every constructed set is balanced
            let s: i128 = set
                .postings()
                .iter()
                .map(|p| i128::from(p.delta.minor()))
                .sum();
            assert_eq!(s, 0);
            let before = b.clone();
            match apply(&mut b, &set) {
                Ok(()) => {
                    ok += 1;
                    if set.reason() == Reason::Deposit {
                        deposited += i128::from(amount.minor());
                    }
                }
                Err(_) => {
                    rejected += 1;
                    assert_eq!(b, before, "a rejected set must change nothing");
                }
            }
            invariants(&b, deposited);
        }
        assert!(
            ok > 50 && rejected > 10,
            "seed {seed}: ok={ok} rejected={rejected}"
        );
    }
}

#[test]
fn overdraw_by_one_unit_is_rejected_everywhere() {
    let a = id(1);
    let mut b = Balances::new();
    apply(&mut b, &deposit(Env::Simulated, m(100), "d").unwrap()).unwrap();
    assert!(apply(&mut b, &allocate(Env::Simulated, a, m(101), "x1").unwrap()).is_err());
    apply(&mut b, &allocate(Env::Simulated, a, m(100), "x2").unwrap()).unwrap();
    assert!(apply(&mut b, &allocate(Env::Simulated, a, m(1), "x3").unwrap()).is_err());
    assert!(
        apply(
            &mut b,
            &commit_stake(Env::Simulated, a, id(9), m(101)).unwrap()
        )
        .is_err()
    );
    assert!(apply(&mut b, &refund(Env::Simulated, a, id(9), m(1)).unwrap()).is_err());
    assert!(
        apply(
            &mut b,
            &settle(Env::Simulated, id(9), a, id(2), m(1)).unwrap()
        )
        .is_err()
    );
    invariants(&b, 100);
}

#[test]
fn malformed_posting_sets_are_rejected() {
    let acct = |n| Account::AgentAvailable(id(n));
    let p = |account, delta: i64| Posting {
        account,
        delta: SignedMoney::from_minor(delta),
    };
    let new =
        |k: &str, ps: Vec<Posting>| PostingSet::new(Env::Simulated, Reason::Deposit, k, None, ps);

    assert_eq!(new("k", vec![]).unwrap_err(), LedgerError::NoPostings);
    assert_eq!(
        new("", vec![p(acct(1), 1), p(acct(2), -1)]).unwrap_err(),
        LedgerError::EmptyIdempotencyKey
    );
    assert_eq!(
        new("  \t", vec![p(acct(1), 1), p(acct(2), -1)]).unwrap_err(),
        LedgerError::EmptyIdempotencyKey
    );
    assert_eq!(
        new("k", vec![p(acct(1), 0), p(acct(2), 0)]).unwrap_err(),
        LedgerError::ZeroAmount
    );
    assert_eq!(
        new("k", vec![p(acct(1), 5)]).unwrap_err(),
        LedgerError::Unbalanced
    );
    assert_eq!(
        new("k", vec![p(acct(1), 5), p(acct(2), -4)]).unwrap_err(),
        LedgerError::Unbalanced
    );
    // sum overflows i64 but is balanced in i128 terms per pair: must be Overflow, never wrap
    assert_eq!(
        new(
            "k",
            vec![p(acct(1), i64::MAX), p(acct(1), 1), p(acct(2), i64::MIN)]
        )
        .unwrap_err(),
        LedgerError::Overflow
    );
    // i64::MIN must not be negatable by accident
    assert!(new("k", vec![p(acct(1), i64::MIN), p(acct(2), i64::MAX)]).is_err());
}

#[test]
fn zero_and_same_party_are_rejected_by_constructors() {
    let a = id(1);
    assert_eq!(
        deposit(Env::Simulated, Money::ZERO, "k").unwrap_err(),
        LedgerError::ZeroAmount
    );
    assert_eq!(
        allocate(Env::Simulated, a, Money::ZERO, "k").unwrap_err(),
        LedgerError::ZeroAmount
    );
    assert_eq!(
        settle(Env::Simulated, id(5), a, a, m(1)).unwrap_err(),
        LedgerError::SameParty
    );
    // double stake overflow in settle
    assert_eq!(
        settle(Env::Simulated, id(5), a, id(2), Money::MAX).unwrap_err(),
        LedgerError::Overflow
    );
}

#[test]
fn balance_overflow_is_an_error_not_a_wrap() {
    let mut b = Balances::new();
    b.insert(Account::UserUnallocated, SignedMoney::from_minor(i64::MAX));
    let set = deposit(Env::Simulated, m(1), "k").unwrap();
    assert_eq!(set.check_balances(&b).unwrap_err(), LedgerError::Overflow);
}

/// SEC-03: settlement used to be guarded only per agent account, so settling
/// the SAME interaction twice under two caller-chosen idempotency keys drained
/// stakes that belonged to other interactions. The key is now derived from
/// the interaction, so every settle of one interaction is the same posting
/// set identity and a store that deduplicates on the key applies it once.
/// (The server adds a row lock on the interaction and unique indexes; see
/// `settling_twice_is_rejected` in the server tests.)
#[test]
fn settle_twice_is_rejected_by_key_derivation() {
    let (w, l, ix) = (id(1), id(2), id(50));
    let first = settle(Env::Simulated, ix, w, l, m(100)).unwrap();
    let second = settle(Env::Simulated, ix, w, l, m(100)).unwrap();
    assert_eq!(first.idempotency_key(), second.idempotency_key());
    assert_eq!(first.idempotency_key(), format!("settle:{ix}"));
    // Swapping winner and loser, or changing the stake, cannot mint a new key.
    assert_eq!(
        settle(Env::Simulated, ix, l, w, m(1))
            .unwrap()
            .idempotency_key(),
        first.idempotency_key()
    );
    // A different interaction is a different key.
    assert_ne!(
        settle(Env::Simulated, id(51), w, l, m(100))
            .unwrap()
            .idempotency_key(),
        first.idempotency_key()
    );

    // Refunds and stake commits are keyed per (interaction, agent).
    let r = |ag| refund(Env::Simulated, ag, ix, m(100)).unwrap();
    assert_eq!(r(w).idempotency_key(), r(w).idempotency_key());
    assert_ne!(r(w).idempotency_key(), r(l).idempotency_key());
    let c = |ag| commit_stake(Env::Simulated, ag, ix, m(100)).unwrap();
    assert_eq!(c(w).idempotency_key(), c(w).idempotency_key());
    assert_ne!(c(w).idempotency_key(), c(l).idempotency_key());

    // A key-deduplicating store therefore applies the settlement once.
    let mut b = Balances::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut apply_once = |b: &mut Balances, set: &PostingSet| -> bool {
        if !seen.insert(set.idempotency_key().to_owned()) {
            return false;
        }
        apply(b, set).unwrap();
        true
    };
    assert!(apply_once(
        &mut b,
        &deposit(Env::Simulated, m(1000), "d").unwrap()
    ));
    for (n, ag) in [(1, w), (2, l)] {
        assert!(apply_once(
            &mut b,
            &allocate(Env::Simulated, ag, m(200), &format!("a{n}")).unwrap()
        ));
        assert!(apply_once(
            &mut b,
            &commit_stake(Env::Simulated, ag, ix, m(100)).unwrap()
        ));
        assert!(apply_once(
            &mut b,
            &commit_stake(Env::Simulated, ag, id(51), m(100)).unwrap()
        ));
    }
    assert!(apply_once(&mut b, &first));
    assert!(
        !apply_once(&mut b, &second),
        "second settle must not be applied"
    );
    // The other interaction's stakes are untouched.
    for ag in [w, l] {
        assert_eq!(
            b[&Account::AgentCommitted(ag)],
            SignedMoney::from_minor(100)
        );
    }
    invariants(&b, 1000);
}
