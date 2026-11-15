-- Starter templates. Amounts are USDC minor units (6 decimals) as strings.
-- The integration tests check that every graph and policy here validates.

INSERT INTO templates (id, position, name, description, config, policy, graph) VALUES
(
    '00000000-0000-4000-8000-000000000001', 1,
    'Momentum bull',
    'Proposes challenges when a market moves sharply and conviction is high. Small stakes go out automatically, larger ones ask you first.',
    '{
        "goals": "Grow the balance by backing strong upward moves.",
        "strategy": "Follow momentum: when a market moves up quickly, expect it to continue over the short term.",
        "categories": ["crypto"],
        "horizon_secs": 3600,
        "risk_tolerance": "high",
        "schedule_secs": 60
    }',
    '{
        "max_per_interaction": "10000000",
        "max_aggregate_exposure": "50000000",
        "approval_threshold": "5000000",
        "allowed_interaction_types": ["challenge"],
        "min_conviction_bps": 6000,
        "stop_loss": "20000000"
    }',
    '{
        "schema_version": 1,
        "nodes": [
            {"id": "trigger", "type": "trigger.market_move", "params": {"symbol": "BTCUSD", "threshold_bps": 200, "window_secs": 3600}, "ui": {"x": 0, "y": 120}},
            {"id": "evaluate", "type": "reason.evaluate_opportunity", "params": {}, "ui": {"x": 260, "y": 120}},
            {"id": "conviction", "type": "cond.conviction_gte", "params": {"min_bps": 6000}, "ui": {"x": 520, "y": 120}},
            {"id": "generate", "type": "reason.generate_proposal", "params": {}, "ui": {"x": 780, "y": 120}},
            {"id": "stake", "type": "cond.stake_lte", "params": {"max": "5000000"}, "ui": {"x": 1040, "y": 120}},
            {"id": "propose", "type": "action.propose_challenge", "params": {}, "ui": {"x": 1300, "y": 40}},
            {"id": "ask", "type": "action.request_approval", "params": {"note": "Stake is above my automatic limit."}, "ui": {"x": 1300, "y": 200}}
        ],
        "edges": [
            {"from": "trigger", "from_port": "out", "to": "evaluate", "to_port": "in"},
            {"from": "evaluate", "from_port": "out", "to": "conviction", "to_port": "in"},
            {"from": "conviction", "from_port": "true", "to": "generate", "to_port": "in"},
            {"from": "generate", "from_port": "out", "to": "stake", "to_port": "in"},
            {"from": "stake", "from_port": "true", "to": "propose", "to_port": "in"},
            {"from": "stake", "from_port": "false", "to": "ask", "to_port": "in"}
        ]
    }'
),
(
    '00000000-0000-4000-8000-000000000002', 2,
    'Skeptic',
    'Waits for other agents to propose challenges. Accepts the ones it strongly disagrees with, counters the rest, rejects anything off-strategy.',
    '{
        "goals": "Profit from other agents'' overconfidence.",
        "strategy": "Take the other side of aggressive claims; most sharp moves revert.",
        "categories": ["crypto"],
        "horizon_secs": 3600,
        "risk_tolerance": "medium",
        "schedule_secs": 60
    }',
    '{
        "max_per_interaction": "10000000",
        "max_aggregate_exposure": "30000000",
        "approval_threshold": "10000000",
        "allowed_interaction_types": ["challenge"],
        "min_conviction_bps": 7000,
        "stop_loss": "15000000"
    }',
    '{
        "schema_version": 1,
        "nodes": [
            {"id": "trigger", "type": "trigger.challenge_received", "params": {}, "ui": {"x": 0, "y": 120}},
            {"id": "evaluate", "type": "reason.evaluate_proposal", "params": {}, "ui": {"x": 260, "y": 120}},
            {"id": "strategy", "type": "cond.matches_strategy", "params": {}, "ui": {"x": 520, "y": 120}},
            {"id": "conviction", "type": "cond.conviction_gte", "params": {"min_bps": 7000}, "ui": {"x": 780, "y": 60}},
            {"id": "accept", "type": "action.accept", "params": {}, "ui": {"x": 1040, "y": 0}},
            {"id": "counter", "type": "action.counter", "params": {}, "ui": {"x": 1040, "y": 120}},
            {"id": "reject", "type": "action.reject", "params": {"reason": "Does not fit my strategy."}, "ui": {"x": 780, "y": 240}}
        ],
        "edges": [
            {"from": "trigger", "from_port": "out", "to": "evaluate", "to_port": "in"},
            {"from": "evaluate", "from_port": "out", "to": "strategy", "to_port": "in"},
            {"from": "strategy", "from_port": "true", "to": "conviction", "to_port": "in"},
            {"from": "strategy", "from_port": "false", "to": "reject", "to_port": "in"},
            {"from": "conviction", "from_port": "true", "to": "accept", "to_port": "in"},
            {"from": "conviction", "from_port": "false", "to": "counter", "to_port": "in"}
        ]
    }'
),
(
    '00000000-0000-4000-8000-000000000003', 3,
    'Cautious',
    'Checks the market on a schedule and only acts on very high conviction while exposure is low. Every proposal needs your approval.',
    '{
        "goals": "Preserve capital; take only the clearest opportunities.",
        "strategy": "Act rarely. Prefer small, well-defined challenges with short horizons.",
        "categories": ["crypto"],
        "horizon_secs": 1800,
        "risk_tolerance": "low",
        "schedule_secs": 300
    }',
    '{
        "max_per_interaction": "2000000",
        "max_aggregate_exposure": "10000000",
        "approval_threshold": "0",
        "allowed_interaction_types": ["challenge"],
        "min_conviction_bps": 8000,
        "stop_loss": "5000000"
    }',
    '{
        "schema_version": 1,
        "nodes": [
            {"id": "trigger", "type": "trigger.schedule", "params": {"every_secs": 300}, "ui": {"x": 0, "y": 120}},
            {"id": "evaluate", "type": "reason.evaluate_opportunity", "params": {}, "ui": {"x": 260, "y": 120}},
            {"id": "exposure", "type": "cond.exposure_within", "params": {"max": "6000000"}, "ui": {"x": 520, "y": 120}},
            {"id": "conviction", "type": "cond.conviction_gte", "params": {"min_bps": 8000}, "ui": {"x": 780, "y": 120}},
            {"id": "generate", "type": "reason.generate_proposal", "params": {}, "ui": {"x": 1040, "y": 120}},
            {"id": "ask", "type": "action.request_approval", "params": {}, "ui": {"x": 1300, "y": 120}}
        ],
        "edges": [
            {"from": "trigger", "from_port": "out", "to": "evaluate", "to_port": "in"},
            {"from": "evaluate", "from_port": "out", "to": "exposure", "to_port": "in"},
            {"from": "exposure", "from_port": "true", "to": "conviction", "to_port": "in"},
            {"from": "conviction", "from_port": "true", "to": "generate", "to_port": "in"},
            {"from": "generate", "from_port": "out", "to": "ask", "to_port": "in"}
        ]
    }'
);
