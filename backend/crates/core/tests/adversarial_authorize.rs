//! Adversarial tests for `authorize()` and money parsing: boundaries, zero,
//! overflow, malformed input, and a seeded randomized check that no `Allow`
//! decision can ever violate the policy.

use aen_core::authorize::{AgentStatus, Decision, DenyReason, Intent, Snapshot, authorize};
use aen_core::interaction::{ALL_STATES, InteractionState};
use aen_core::money::{Money, SignedMoney};
use aen_core::policy::{AgentPolicy, InteractionType};

/// xorshift64*: deterministic, no dependencies.
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
    /// Biased towards boundary values.
    fn amount(&mut self) -> i64 {
        match self.below(8) {
            0 => 0,
            1 => 1,
            2 => i64::MAX,
            3 => i64::MAX - 1,
            4 => (self.next() >> 1) as i64,
            _ => self.below(1_000) as i64,
        }
    }
    fn money(&mut self) -> Money {
        Money::from_minor(self.amount()).unwrap()
    }
}

fn m(u: i64) -> Money {
    Money::from_minor(u).unwrap()
}

fn policy() -> AgentPolicy {
    AgentPolicy {
        max_per_interaction: m(100),
        max_aggregate_exposure: m(300),
        approval_threshold: m(50),
        allowed_interaction_types: vec![InteractionType::Challenge],
        min_conviction_bps: 5000,
        stop_loss: Some(m(200)),
    }
}

fn snap() -> Snapshot {
    Snapshot {
        status: AgentStatus::Active,
        available: m(1000),
        exposure: Money::ZERO,
        realized_pnl: SignedMoney::ZERO,
    }
}

fn denied(d: &Decision, r: DenyReason) -> bool {
    matches!(d, Decision::Deny(rs) if rs.contains(&r))
}

#[test]
fn zero_stake_is_denied_for_every_staking_intent() {
    let z = Money::ZERO;
    let intents = [
        Intent::ProposeChallenge { stake: z },
        Intent::ProposeCollaboration { contribution: z },
    ];
    for i in intents {
        assert!(denied(
            &authorize(&i, &policy(), &snap(), None),
            DenyReason::ZeroAmount
        ));
    }
    for (i, st) in [
        (
            Intent::AcceptChallenge { stake: z },
            InteractionState::Proposed,
        ),
        (
            Intent::CounterOffer { stake: z },
            InteractionState::Negotiating,
        ),
        (
            Intent::CommitFunds {
                interaction_type: InteractionType::Challenge,
                stake: z,
            },
            InteractionState::Accepted,
        ),
    ] {
        assert!(denied(
            &authorize(&i, &policy(), &snap(), Some(st)),
            DenyReason::ZeroAmount
        ));
    }
}

#[test]
fn exact_boundaries_allowed_one_over_denied() {
    let mut s = snap();
    s.exposure = m(200);
    // max_per = 100, aggregate headroom = 100, available = 1000
    let p = policy();
    let ok = authorize(&Intent::ProposeChallenge { stake: m(100) }, &p, &s, None);
    assert_eq!(ok, Decision::RequireApproval); // 100 > threshold 50
    let over = authorize(&Intent::ProposeChallenge { stake: m(101) }, &p, &s, None);
    assert!(denied(&over, DenyReason::ExceedsMaxPerInteraction));
    assert!(denied(&over, DenyReason::ExceedsAggregateExposure));
    s.exposure = m(201);
    let over = authorize(&Intent::ProposeChallenge { stake: m(100) }, &p, &s, None);
    assert!(denied(&over, DenyReason::ExceedsAggregateExposure));
    // available boundary
    let mut s = snap();
    s.available = m(10);
    assert_eq!(
        authorize(&Intent::ProposeChallenge { stake: m(10) }, &p, &s, None),
        Decision::Allow
    );
    assert!(denied(
        &authorize(&Intent::ProposeChallenge { stake: m(11) }, &p, &s, None),
        DenyReason::InsufficientAvailableBalance
    ));
}

#[test]
fn overflowing_exposure_is_denied_not_wrapped() {
    let mut p = policy();
    p.max_per_interaction = Money::MAX;
    p.max_aggregate_exposure = Money::MAX;
    p.approval_threshold = Money::MAX;
    let mut s = snap();
    s.available = Money::MAX;
    s.exposure = Money::MAX;
    let d = authorize(&Intent::ProposeChallenge { stake: m(1) }, &p, &s, None);
    assert!(denied(&d, DenyReason::AmountOverflow), "{d:?}");
    // stake == MAX with zero exposure is arithmetically fine
    s.exposure = Money::ZERO;
    let d = authorize(&Intent::ProposeChallenge { stake: Money::MAX }, &p, &s, None);
    assert_eq!(d, Decision::Allow);
}

#[test]
fn stop_loss_boundary_and_extremes() {
    let p = policy(); // stop at 200
    let mut s = snap();
    s.realized_pnl = SignedMoney::from_minor(-199);
    assert_eq!(
        authorize(&Intent::ProposeChallenge { stake: m(10) }, &p, &s, None),
        Decision::Allow
    );
    s.realized_pnl = SignedMoney::from_minor(-200);
    assert!(denied(
        &authorize(&Intent::ProposeChallenge { stake: m(10) }, &p, &s, None),
        DenyReason::StopLossTriggered
    ));
    s.realized_pnl = SignedMoney::from_minor(i64::MIN);
    assert!(denied(
        &authorize(&Intent::ProposeChallenge { stake: m(10) }, &p, &s, None),
        DenyReason::StopLossTriggered
    ));
    // stop_loss = MAX must not overflow when negated
    let mut p2 = policy();
    p2.stop_loss = Some(Money::MAX);
    s.realized_pnl = SignedMoney::from_minor(i64::MIN + 1);
    assert!(denied(
        &authorize(&Intent::ProposeChallenge { stake: m(10) }, &p2, &s, None),
        DenyReason::StopLossTriggered
    ));
    // Reject is never blocked by stop loss
    assert_eq!(
        authorize(&Intent::Reject, &p, &s, Some(InteractionState::Proposed)),
        Decision::Allow
    );
}

#[test]
fn paused_agent_can_do_nothing_including_reject() {
    let mut s = snap();
    s.status = AgentStatus::Paused;
    assert!(denied(
        &authorize(&Intent::Reject, &policy(), &s, Some(InteractionState::Proposed)),
        DenyReason::AgentPaused
    ));
}

#[test]
fn commit_funds_cannot_smuggle_a_disallowed_type() {
    // Policy allows only Challenge; CommitFunds claims Collaboration.
    let d = authorize(
        &Intent::CommitFunds {
            interaction_type: InteractionType::Collaboration,
            stake: m(10),
        },
        &policy(),
        &snap(),
        Some(InteractionState::Accepted),
    );
    assert!(denied(&d, DenyReason::InteractionTypeNotAllowed));
}

#[test]
fn empty_allowed_types_denies_every_typed_intent() {
    let mut p = policy();
    p.allowed_interaction_types.clear();
    let d = authorize(&Intent::ProposeChallenge { stake: m(1) }, &p, &snap(), None);
    assert!(denied(&d, DenyReason::InteractionTypeNotAllowed));
}

#[test]
fn every_intent_in_every_illegal_state_is_denied() {
    let p = policy();
    let s = snap();
    let intents = [
        Intent::ProposeChallenge { stake: m(1) },
        Intent::AcceptChallenge { stake: m(1) },
        Intent::CounterOffer { stake: m(1) },
        Intent::Reject,
        Intent::CommitFunds {
            interaction_type: InteractionType::Challenge,
            stake: m(1),
        },
        Intent::ProposeCollaboration { contribution: m(1) },
    ];
    for i in intents {
        for st in ALL_STATES.iter().copied().map(Some).chain([None]) {
            let d = authorize(&i, &p, &s, st);
            if !i.legal_in(st) {
                assert!(denied(&d, DenyReason::IllegalInteractionState), "{i:?} {st:?}");
            }
        }
    }
    // terminal states accept nothing
    for st in [
        InteractionState::Settled,
        InteractionState::Cancelled,
        InteractionState::Failed,
        InteractionState::Disputed,
    ] {
        for i in intents {
            assert!(matches!(authorize(&i, &p, &s, Some(st)), Decision::Deny(_)));
        }
    }
}

/// Randomized: an Allow/RequireApproval decision must satisfy every limit.
#[test]
fn randomized_allow_never_violates_policy() {
    let mut rng = Rng(0x00C0_FFEE_1234_5678);
    let mut allowed = 0u32;
    for _ in 0..200_000 {
        let p = AgentPolicy {
            max_per_interaction: rng.money(),
            max_aggregate_exposure: rng.money(),
            approval_threshold: rng.money(),
            allowed_interaction_types: match rng.below(4) {
                0 => vec![],
                1 => vec![InteractionType::Challenge],
                2 => vec![InteractionType::Collaboration],
                _ => vec![InteractionType::Challenge, InteractionType::Collaboration],
            },
            min_conviction_bps: 0,
            stop_loss: if rng.below(2) == 0 { None } else { Some(rng.money()) },
        };
        let s = Snapshot {
            status: if rng.below(5) == 0 { AgentStatus::Paused } else { AgentStatus::Active },
            available: rng.money(),
            exposure: rng.money(),
            realized_pnl: SignedMoney::from_minor(match rng.below(3) {
                0 => i64::MIN,
                1 => -rng.amount(),
                _ => rng.amount(),
            }),
        };
        let stake = rng.money();
        let kind = if rng.below(2) == 0 { InteractionType::Challenge } else { InteractionType::Collaboration };
        let (intent, state) = match rng.below(5) {
            0 => (Intent::ProposeChallenge { stake }, None),
            1 => (Intent::ProposeCollaboration { contribution: stake }, None),
            2 => (Intent::AcceptChallenge { stake }, Some(InteractionState::Proposed)),
            3 => (Intent::CommitFunds { interaction_type: kind, stake }, Some(InteractionState::Accepted)),
            _ => (Intent::CounterOffer { stake }, Some(InteractionState::Negotiating)),
        };
        let d = authorize(&intent, &p, &s, state);
        if matches!(d, Decision::Allow | Decision::RequireApproval) {
            allowed += 1;
            assert_eq!(s.status, AgentStatus::Active);
            assert!(!stake.is_zero());
            assert!(stake <= p.max_per_interaction);
            assert!(stake <= s.available);
            let total = s.exposure.checked_add(stake).expect("no overflow on allow");
            assert!(total <= p.max_aggregate_exposure);
            assert!(p.allowed_interaction_types.contains(&intent.interaction_type().unwrap()));
            if let Some(limit) = p.stop_loss {
                assert!(s.realized_pnl > limit.debit());
            }
            assert_eq!(d == Decision::RequireApproval, stake > p.approval_threshold);
        }
    }
    assert!(allowed > 100, "generator too restrictive: {allowed}");
}

#[test]
fn unvalidated_policy_with_inverted_limits_still_enforces_each_limit() {
    // validate() would reject this, but authorize must not depend on it.
    let p = AgentPolicy {
        max_per_interaction: m(500),
        max_aggregate_exposure: m(100),
        approval_threshold: m(900),
        allowed_interaction_types: vec![InteractionType::Challenge],
        min_conviction_bps: 99_999,
        stop_loss: Some(Money::ZERO),
    };
    assert!(!p.validate().is_empty());
    let d = authorize(&Intent::ProposeChallenge { stake: m(200) }, &p, &snap(), None);
    assert!(denied(&d, DenyReason::ExceedsAggregateExposure));
}

// ---- money parsing ----

#[test]
fn money_rejects_malformed_strings() {
    for bad in [
        "", " ", "-1", "+1", "01", "1.0", "1e3", "0x10", " 1", "1 ", "1\n", "١٢٣", "１２", "9223372036854775808",
        "99999999999999999999999999", "NaN", "null", "1_000", "--1", "-",
    ] {
        assert!(bad.parse::<Money>().is_err(), "accepted {bad:?}");
    }
    assert_eq!("0".parse::<Money>().unwrap(), Money::ZERO);
    assert_eq!("9223372036854775807".parse::<Money>().unwrap(), Money::MAX);
}

#[test]
fn signed_money_rejects_noncanonical_negatives() {
    for bad in ["-0", "-01", "-", "--1", "-+1", "- 1", "-9223372036854775809", "+0"] {
        assert!(bad.parse::<SignedMoney>().is_err(), "accepted {bad:?}");
    }
    assert_eq!(
        "-9223372036854775808".parse::<SignedMoney>().unwrap().minor(),
        i64::MIN
    );
}

#[test]
fn money_json_must_be_a_string() {
    for bad in ["5", "5.5", "-5", "null", "true", "[]", "{}", "\"-5\"", "\"5.0\"", "\"\""] {
        assert!(serde_json::from_str::<Money>(bad).is_err(), "accepted {bad}");
    }
    let intent: Result<Intent, _> =
        serde_json::from_str(r#"{"type":"propose_challenge","stake":-1}"#);
    assert!(intent.is_err());
    let unknown: Result<Intent, _> = serde_json::from_str(r#"{"type":"withdraw_all","stake":"1"}"#);
    assert!(unknown.is_err());
}

#[test]
fn randomized_money_parse_never_panics_and_round_trips() {
    let mut rng = Rng(42);
    let alphabet: Vec<char> = "0123456789-+. eE_x\u{0663}\u{ff11}\n".chars().collect();
    for _ in 0..50_000 {
        let len = rng.below(25) as usize;
        let s: String = (0..len).map(|_| alphabet[rng.below(alphabet.len() as u64) as usize]).collect();
        if let Ok(v) = s.parse::<Money>() {
            assert_eq!(v.to_string(), s);
            assert!(v.minor() >= 0);
        }
        if let Ok(v) = s.parse::<SignedMoney>() {
            assert_eq!(v.to_string(), s);
        }
    }
}
