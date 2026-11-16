//! Behavior graph: the visual builder's output.
//!
//! The block catalog and the validator live here; the frontend renders blocks
//! from the catalog and never defines them. Conditions in a graph are owner
//! preferences, not security: `authorize` applies the policy regardless.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::OnceLock;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const SCHEMA_VERSION: u32 = 1;

/// User JavaScript `decide(ctx)`, run in the sandbox (D-11). An action: its
/// output is intents, which `authorize` judges like any other.
pub const CUSTOM_CODE: &str = "custom.code";
/// Largest `custom.code` source, in UTF-8 bytes (D-41).
pub const MAX_CUSTOM_CODE_BYTES: usize = 64 << 10;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Graph {
    pub schema_version: u32,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Node {
    pub id: String,
    /// A block type from the catalog, e.g. `trigger.schedule`.
    #[serde(rename = "type")]
    pub node_type: String,
    /// Must satisfy the block's `params_schema`.
    #[serde(default = "empty_object")]
    pub params: Value,
    /// Canvas position, owned by the builder. Stored and returned as-is;
    /// the validator ignores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ui: Option<NodeUi>,
}

/// Presentation only (not money): where the node sits on the canvas.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NodeUi {
    pub x: f64,
    pub y: f64,
}

fn empty_object() -> Value {
    json!({})
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Edge {
    pub from: String,
    pub from_port: String,
    pub to: String,
    pub to_port: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockKind {
    Trigger,
    Reasoning,
    Condition,
    Action,
}

/// What flows along an edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataType {
    /// What a trigger observed (a tick or a market move).
    Event,
    /// A reasoning result: conviction, strategy fit, suggested stake.
    Assessment,
    /// Explicit challenge terms, received or generated.
    Proposal,
    /// Input ports only: accepts any of the above.
    Any,
}

impl DataType {
    /// Whether an output of type `self` may feed an input of type `input`.
    pub fn feeds(self, input: DataType) -> bool {
        input == DataType::Any || self == input
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortDef {
    pub port: String,
    pub data_type: DataType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlockDef {
    #[serde(rename = "type")]
    pub block_type: String,
    pub kind: BlockKind,
    pub label: String,
    pub description: String,
    pub inputs: Vec<PortDef>,
    pub outputs: Vec<PortDef>,
    pub params_schema: Value,
}

fn ports(defs: &[(&str, DataType)]) -> Vec<PortDef> {
    defs.iter()
        .map(|(port, data_type)| PortDef {
            port: (*port).to_owned(),
            data_type: *data_type,
        })
        .collect()
}

fn params(properties: Value, required: &[&str]) -> Value {
    json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false,
    })
}

fn block(
    block_type: &str,
    kind: BlockKind,
    label: &str,
    description: &str,
    inputs: &[(&str, DataType)],
    outputs: &[(&str, DataType)],
    params_schema: Value,
) -> BlockDef {
    BlockDef {
        block_type: block_type.to_owned(),
        kind,
        label: label.to_owned(),
        description: description.to_owned(),
        inputs: ports(inputs),
        outputs: ports(outputs),
        params_schema,
    }
}

/// The block catalog: every node type a graph may use.
pub fn catalog() -> &'static [BlockDef] {
    static CATALOG: OnceLock<Vec<BlockDef>> = OnceLock::new();
    CATALOG.get_or_init(build_catalog)
}

pub fn block_def(block_type: &str) -> Option<&'static BlockDef> {
    catalog().iter().find(|b| b.block_type == block_type)
}

fn build_catalog() -> Vec<BlockDef> {
    use BlockKind::*;
    use DataType::*;

    // Integer USDC minor units as a string, same as everywhere else in the API.
    let money = json!({
        "type": "string",
        "pattern": "^(0|[1-9][0-9]{0,17})$",
        "description": "USDC minor units (6 decimals)",
    });
    let bps = json!({ "type": "integer", "minimum": 0, "maximum": 10000 });
    let instructions = json!({ "instructions": { "type": "string", "maxLength": 2000 } });
    let branch = |t: DataType| [("true", t), ("false", t)];

    vec![
        block(
            "trigger.schedule",
            Trigger,
            "On a schedule",
            "Runs the flow at a fixed interval.",
            &[],
            &[("out", Event)],
            params(
                json!({ "every_secs": { "type": "integer", "minimum": 1, "maximum": 86400 } }),
                &["every_secs"],
            ),
        ),
        block(
            "trigger.market_move",
            Trigger,
            "On a market move",
            "Runs when a market moves by at least the threshold within the window.",
            &[],
            &[("out", Event)],
            params(
                json!({
                    "symbol": { "type": "string", "minLength": 1, "maxLength": 64 },
                    "threshold_bps": { "type": "integer", "minimum": 1, "maximum": 10000 },
                    "window_secs": { "type": "integer", "minimum": 1, "maximum": 604800 },
                }),
                &["symbol", "threshold_bps", "window_secs"],
            ),
        ),
        block(
            "trigger.challenge_received",
            Trigger,
            "On a challenge received",
            "Runs when another agent proposes a challenge to this agent.",
            &[],
            &[("out", Proposal)],
            params(json!({}), &[]),
        ),
        block(
            "reason.evaluate_opportunity",
            Reasoning,
            "Evaluate opportunity",
            "Assesses the triggering event against the agent's goals and strategy.",
            &[("in", Event)],
            &[("out", Assessment)],
            params(instructions.clone(), &[]),
        ),
        block(
            "reason.evaluate_proposal",
            Reasoning,
            "Evaluate proposal",
            "Assesses a received challenge proposal.",
            &[("in", Proposal)],
            &[("out", Assessment)],
            params(instructions.clone(), &[]),
        ),
        block(
            "reason.generate_proposal",
            Reasoning,
            "Generate proposal",
            "Turns an assessment into explicit, measurable challenge terms.",
            &[("in", Assessment)],
            &[("out", Proposal)],
            params(instructions, &[]),
        ),
        block(
            "cond.conviction_gte",
            Condition,
            "Conviction at least",
            "Passes when the assessment's conviction is at least the minimum.",
            &[("in", Assessment)],
            &branch(Assessment),
            params(json!({ "min_bps": bps }), &["min_bps"]),
        ),
        block(
            "cond.exposure_within",
            Condition,
            "Exposure within",
            "Passes when the agent's current exposure is at most the maximum. \
             A preference only: the agent's policy is always enforced separately.",
            &[("in", Assessment)],
            &branch(Assessment),
            params(json!({ "max": money }), &["max"]),
        ),
        block(
            "cond.matches_strategy",
            Condition,
            "Matches strategy",
            "Passes when the assessment fits the agent's stated strategy.",
            &[("in", Assessment)],
            &branch(Assessment),
            params(json!({}), &[]),
        ),
        block(
            "cond.stake_lte",
            Condition,
            "Stake at most",
            "Passes when the proposal's stake is at most the maximum.",
            &[("in", Proposal)],
            &branch(Proposal),
            params(json!({ "max": money }), &["max"]),
        ),
        block(
            "action.propose_challenge",
            Action,
            "Propose challenge",
            "Proposes the challenge to a counterparty.",
            &[("in", Proposal)],
            &[],
            params(json!({}), &[]),
        ),
        block(
            "action.accept",
            Action,
            "Accept",
            "Accepts the assessed proposal.",
            &[("in", Assessment)],
            &[],
            params(json!({}), &[]),
        ),
        block(
            "action.counter",
            Action,
            "Counter-offer",
            "Responds to the assessed proposal with different terms.",
            &[("in", Assessment)],
            &[],
            params(json!({}), &[]),
        ),
        block(
            "action.reject",
            Action,
            "Reject",
            "Declines and ends the flow.",
            &[("in", Any)],
            &[],
            params(
                json!({ "reason": { "type": "string", "maxLength": 500 } }),
                &[],
            ),
        ),
        block(
            "action.request_approval",
            Action,
            "Ask owner",
            "Sends the proposal or assessment to the owner for a decision.",
            &[("in", Any)],
            &[],
            params(
                json!({ "note": { "type": "string", "maxLength": 500 } }),
                &[],
            ),
        ),
        block(
            CUSTOM_CODE,
            Action,
            "Custom code",
            "Runs your JavaScript `function decide(ctx)` in a sandbox (no network, no keys, \
             fixed CPU, memory and time). It returns a list of intents such as \
             {\"type\": \"propose_challenge\", \"stake\": \"2500000\"}; each one is checked \
             against the agent's policy exactly like a built-in block's.",
            &[("in", Any)],
            &[],
            params(
                json!({ "source": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": MAX_CUSTOM_CODE_BYTES,
                    "description": "JavaScript defining function decide(ctx); at most 64 KiB",
                } }),
                &["source"],
            ),
        ),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphErrorCode {
    UnsupportedSchemaVersion,
    InvalidNodeId,
    DuplicateNodeId,
    UnknownNodeType,
    InvalidParams,
    NoTrigger,
    MultipleTriggers,
    UnknownEdgeNode,
    UnknownPort,
    IncompatiblePorts,
    Cycle,
    UnreachableNode,
    DeadEnd,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphError {
    pub code: GraphErrorCode,
    pub message: String,
    pub node_id: Option<String>,
}

fn err(code: GraphErrorCode, node_id: Option<&str>, message: String) -> GraphError {
    GraphError {
        code,
        message,
        node_id: node_id.map(str::to_owned),
    }
}

/// Returns every problem with the graph; empty means valid.
///
/// Checks: schema version, unique non-empty node ids, known node types, params
/// against the block's schema, exactly one trigger, edges reference existing
/// nodes and ports with compatible data types, no cycles, every node reachable
/// from the trigger, and every path ends in an action (a node without outgoing
/// edges must be an action).
pub fn validate(graph: &Graph) -> Vec<GraphError> {
    use GraphErrorCode::*;
    let mut errors = Vec::new();

    if graph.schema_version != SCHEMA_VERSION {
        errors.push(err(
            UnsupportedSchemaVersion,
            None,
            format!(
                "schema_version {} is not supported (expected {SCHEMA_VERSION})",
                graph.schema_version
            ),
        ));
    }

    // Nodes. `defs` maps node id -> block definition (None for unknown types).
    let mut defs: HashMap<&str, Option<&'static BlockDef>> = HashMap::new();
    let mut order: Vec<&str> = Vec::new();
    for node in &graph.nodes {
        let id = node.id.as_str();
        if id.trim().is_empty() {
            errors.push(err(InvalidNodeId, None, "node id must not be empty".into()));
            continue;
        }
        if defs.contains_key(id) {
            errors.push(err(
                DuplicateNodeId,
                Some(id),
                format!("node id '{id}' is used more than once"),
            ));
            continue;
        }
        let def = block_def(&node.node_type);
        match def {
            None => errors.push(err(
                UnknownNodeType,
                Some(id),
                format!("unknown node type '{}'", node.node_type),
            )),
            Some(def) => {
                for message in param_errors(def, &node.params) {
                    errors.push(err(
                        InvalidParams,
                        Some(id),
                        format!("invalid params for {}: {message}", def.block_type),
                    ));
                }
                // `maxLength` counts characters; the sandbox limit is bytes.
                // Reported only when the schema did not already flag it.
                let source = node.params.get("source").and_then(Value::as_str);
                if def.block_type == CUSTOM_CODE
                    && source.is_some_and(|s| {
                        s.len() > MAX_CUSTOM_CODE_BYTES
                            && s.chars().count() <= MAX_CUSTOM_CODE_BYTES
                    })
                {
                    errors.push(err(
                        InvalidParams,
                        Some(id),
                        format!(
                            "invalid params for {CUSTOM_CODE}: source is larger than \
                             {MAX_CUSTOM_CODE_BYTES} bytes"
                        ),
                    ));
                }
            }
        }
        defs.insert(id, def);
        order.push(id);
    }

    let triggers: Vec<&str> = order
        .iter()
        .copied()
        .filter(|id| matches!(defs[id], Some(d) if d.kind == BlockKind::Trigger))
        .collect();
    match triggers.as_slice() {
        [] => errors.push(err(
            NoTrigger,
            None,
            "a graph needs exactly one trigger".into(),
        )),
        [_] => {}
        [_, extra @ ..] => {
            for id in extra {
                errors.push(err(
                    MultipleTriggers,
                    Some(id),
                    "a graph needs exactly one trigger; remove this one".into(),
                ));
            }
        }
    }

    // Edges. Every edge between two existing nodes counts for the topology
    // checks below, even if its ports are wrong.
    let mut successors: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut indegree: HashMap<&str, usize> = order.iter().map(|id| (*id, 0)).collect();
    for edge in &graph.edges {
        let (from, to) = (edge.from.as_str(), edge.to.as_str());
        let mut missing = false;
        for (end, other) in [(from, to), (to, from)] {
            if !defs.contains_key(end) {
                missing = true;
                errors.push(err(
                    UnknownEdgeNode,
                    defs.contains_key(other).then_some(other),
                    format!("edge {from} -> {to} references unknown node '{end}'"),
                ));
            }
        }
        if missing {
            continue;
        }
        successors.entry(from).or_default().push(to);
        *indegree.entry(to).or_default() += 1;

        let (Some(from_def), Some(to_def)) = (defs[from], defs[to]) else {
            continue; // unknown type already reported
        };
        let out = from_def.outputs.iter().find(|p| p.port == edge.from_port);
        let input = to_def.inputs.iter().find(|p| p.port == edge.to_port);
        if out.is_none() {
            errors.push(err(
                UnknownPort,
                Some(from),
                format!(
                    "{} has no output port '{}'",
                    from_def.block_type, edge.from_port
                ),
            ));
        }
        if input.is_none() {
            errors.push(err(
                UnknownPort,
                Some(to),
                format!("{} has no input port '{}'", to_def.block_type, edge.to_port),
            ));
        }
        if let (Some(out), Some(input)) = (out, input)
            && !out.data_type.feeds(input.data_type)
        {
            errors.push(err(
                IncompatiblePorts,
                Some(to),
                format!(
                    "{}.{} produces {:?} but {}.{} expects {:?}",
                    from, out.port, out.data_type, to, input.port, input.data_type
                ),
            ));
        }
    }

    // Acyclic: Kahn's algorithm. Whatever cannot be sorted is on or behind a cycle.
    let mut queue: VecDeque<&str> = order
        .iter()
        .copied()
        .filter(|id| indegree[id] == 0)
        .collect();
    let mut sorted = 0;
    while let Some(id) = queue.pop_front() {
        sorted += 1;
        for next in successors.get(id).into_iter().flatten() {
            if let Some(d) = indegree.get_mut(next) {
                *d -= 1;
                if *d == 0 {
                    queue.push_back(next);
                }
            }
        }
    }
    if sorted < order.len() {
        let stuck = order.iter().copied().find(|id| indegree[id] > 0);
        errors.push(err(Cycle, stuck, "the graph contains a cycle".into()));
    }

    // Reachability from the trigger(s). Skipped without a trigger: that is
    // already reported and every node would be flagged.
    if !triggers.is_empty() {
        let mut seen: HashSet<&str> = triggers.iter().copied().collect();
        let mut queue: VecDeque<&str> = triggers.iter().copied().collect();
        while let Some(id) = queue.pop_front() {
            for next in successors.get(id).into_iter().flatten() {
                if seen.insert(next) {
                    queue.push_back(next);
                }
            }
        }
        for id in order.iter().filter(|id| !seen.contains(*id)) {
            errors.push(err(
                UnreachableNode,
                Some(id),
                format!("node '{id}' is not reachable from the trigger"),
            ));
        }
    }

    // Every path ends in an action.
    for id in &order {
        let is_leaf = successors.get(id).is_none_or(Vec::is_empty);
        if let Some(def) = defs[id]
            && is_leaf
            && def.kind != BlockKind::Action
        {
            errors.push(err(
                DeadEnd,
                Some(id),
                format!("node '{id}' leads nowhere; every path must end in an action"),
            ));
        }
    }

    errors
}

fn param_errors(def: &BlockDef, params: &Value) -> Vec<String> {
    match jsonschema::validator_for(&def.params_schema) {
        Ok(validator) => validator
            .iter_errors(params)
            .map(|e| e.to_string())
            .collect(),
        // A broken catalog schema must fail closed, never accept params.
        Err(e) => vec![format!("internal: block schema does not compile: {e}")],
    }
}

/// JSON Schema of the `Graph` document, for `schemas/graph.schema.json`.
pub fn graph_json_schema() -> Value {
    serde_json::to_value(schemars::schema_for!(Graph)).unwrap_or(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    use GraphErrorCode::*;

    fn node(id: &str, node_type: &str, params: Value) -> Node {
        Node {
            id: id.into(),
            node_type: node_type.into(),
            params,
            ui: None,
        }
    }

    fn edge(from: &str, from_port: &str, to: &str) -> Edge {
        Edge {
            from: from.into(),
            from_port: from_port.into(),
            to: to.into(),
            to_port: "in".into(),
        }
    }

    /// market move -> evaluate -> conviction? -> generate -> stake<=? -> propose / ask owner
    fn valid() -> Graph {
        Graph {
            schema_version: 1,
            nodes: vec![
                node(
                    "t",
                    "trigger.market_move",
                    json!({"symbol": "BTCUSD", "threshold_bps": 200, "window_secs": 3600}),
                ),
                node("eval", "reason.evaluate_opportunity", json!({})),
                node("conv", "cond.conviction_gte", json!({"min_bps": 6000})),
                node("gen", "reason.generate_proposal", json!({})),
                node("stake", "cond.stake_lte", json!({"max": "5000000"})),
                node("propose", "action.propose_challenge", json!({})),
                node("ask", "action.request_approval", json!({})),
            ],
            edges: vec![
                edge("t", "out", "eval"),
                edge("eval", "out", "conv"),
                edge("conv", "true", "gen"),
                edge("gen", "out", "stake"),
                edge("stake", "true", "propose"),
                edge("stake", "false", "ask"),
            ],
        }
    }

    fn codes(graph: &Graph) -> Vec<GraphErrorCode> {
        validate(graph).into_iter().map(|e| e.code).collect()
    }

    fn find(graph: &Graph, code: GraphErrorCode) -> GraphError {
        validate(graph)
            .into_iter()
            .find(|e| e.code == code)
            .unwrap_or_else(|| panic!("expected {code:?}, got {:?}", validate(graph)))
    }

    #[test]
    fn valid_graph_passes() {
        assert_eq!(validate(&valid()), vec![]);
    }

    #[test]
    fn catalog_has_the_initial_block_set() {
        let types: Vec<&str> = catalog().iter().map(|b| b.block_type.as_str()).collect();
        assert_eq!(
            types,
            [
                "trigger.schedule",
                "trigger.market_move",
                "trigger.challenge_received",
                "reason.evaluate_opportunity",
                "reason.evaluate_proposal",
                "reason.generate_proposal",
                "cond.conviction_gte",
                "cond.exposure_within",
                "cond.matches_strategy",
                "cond.stake_lte",
                "action.propose_challenge",
                "action.accept",
                "action.counter",
                "action.reject",
                "action.request_approval",
                "custom.code",
            ]
        );
    }

    #[test]
    fn catalog_is_internally_consistent() {
        for def in catalog() {
            let t = &def.block_type;
            assert!(
                jsonschema::validator_for(&def.params_schema).is_ok(),
                "{t}: schema must compile"
            );
            let prefix = match def.kind {
                BlockKind::Trigger => "trigger.",
                BlockKind::Reasoning => "reason.",
                BlockKind::Condition => "cond.",
                BlockKind::Action if t == CUSTOM_CODE => "custom.",
                BlockKind::Action => "action.",
            };
            assert!(t.starts_with(prefix), "{t}");
            match def.kind {
                BlockKind::Trigger => assert!(def.inputs.is_empty() && !def.outputs.is_empty()),
                BlockKind::Action => assert!(!def.inputs.is_empty() && def.outputs.is_empty()),
                _ => assert!(!def.inputs.is_empty() && !def.outputs.is_empty()),
            }
            // `Any` is only meaningful on inputs.
            assert!(def.outputs.iter().all(|p| p.data_type != DataType::Any));
        }
    }

    #[test]
    fn block_def_json_shape() {
        let v = serde_json::to_value(block_def("cond.stake_lte").unwrap()).unwrap();
        assert_eq!(v["type"], "cond.stake_lte");
        assert_eq!(v["kind"], "condition");
        assert_eq!(
            v["inputs"],
            json!([{"port": "in", "data_type": "proposal"}])
        );
        assert_eq!(
            v["outputs"][1],
            json!({"port": "false", "data_type": "proposal"})
        );
        assert_eq!(v["params_schema"]["type"], "object");
    }

    #[test]
    fn graph_json_round_trips_and_keeps_ui() {
        let json = json!({
            "schema_version": 1,
            "nodes": [{"id": "n1", "type": "trigger.schedule", "params": {"every_secs": 60},
                       "ui": {"x": 10.5, "y": -20.25}}],
            "edges": [{"from": "n1", "from_port": "out", "to": "n2", "to_port": "in"}]
        });
        let graph: Graph = serde_json::from_value(json.clone()).unwrap();
        assert_eq!(graph.nodes[0].node_type, "trigger.schedule");
        assert_eq!(serde_json::to_value(&graph).unwrap(), json);
    }

    #[test]
    fn unsupported_schema_version() {
        let mut g = valid();
        g.schema_version = 2;
        assert_eq!(codes(&g), [UnsupportedSchemaVersion]);
    }

    #[test]
    fn empty_graph_has_no_trigger() {
        let g = Graph {
            schema_version: 1,
            nodes: vec![],
            edges: vec![],
        };
        assert_eq!(codes(&g), [NoTrigger]);
    }

    #[test]
    fn unknown_node_type() {
        let mut g = valid();
        g.nodes[1].node_type = "reason.read_tea_leaves".into();
        let e = find(&g, UnknownNodeType);
        assert_eq!(e.node_id.as_deref(), Some("eval"));
        assert_eq!(codes(&g), [UnknownNodeType]);
    }

    fn code_graph(source: &str) -> Graph {
        Graph {
            schema_version: 1,
            nodes: vec![
                node("t", "trigger.schedule", json!({"every_secs": 60})),
                node("code", CUSTOM_CODE, json!({ "source": source })),
            ],
            edges: vec![edge("t", "out", "code")],
        }
    }

    #[test]
    fn custom_code_is_an_action_fed_by_anything() {
        let src = "function decide(ctx) { return []; }";
        assert_eq!(validate(&code_graph(src)), vec![]);
        // After a reasoning step too (any input type).
        let mut g = valid();
        g.nodes
            .push(node("code", CUSTOM_CODE, json!({ "source": src })));
        g.edges.push(edge("conv", "false", "code"));
        assert_eq!(validate(&g), vec![]);
        // Nothing may follow it.
        let mut g = code_graph(src);
        g.nodes.push(node("rej", "action.reject", json!({})));
        g.edges.push(edge("code", "out", "rej"));
        assert_eq!(codes(&g), [UnknownPort]);
    }

    #[test]
    fn custom_code_source_is_required_and_capped_in_bytes() {
        for params in [
            json!({}),
            json!({"source": ""}),
            json!({"source": 5}),
            json!({"source": "x", "extra": 1}),
        ] {
            let mut g = code_graph("x");
            g.nodes[1].params = params.clone();
            let got = codes(&g);
            assert!(
                !got.is_empty() && got.iter().all(|c| *c == InvalidParams),
                "{params}: {got:?}"
            );
        }
        assert_eq!(
            validate(&code_graph(&"x".repeat(MAX_CUSTOM_CODE_BYTES))),
            vec![]
        );
        assert_eq!(
            codes(&code_graph(&"x".repeat(MAX_CUSTOM_CODE_BYTES + 1))),
            [InvalidParams]
        );
        // 3-byte characters: under the character limit, over the byte limit.
        let wide = "\u{20ac}".repeat(MAX_CUSTOM_CODE_BYTES / 3 + 1);
        assert!(wide.chars().count() <= MAX_CUSTOM_CODE_BYTES);
        assert_eq!(codes(&code_graph(&wide)), [InvalidParams]);
    }

    #[test]
    fn duplicate_and_empty_node_ids() {
        let mut g = valid();
        g.nodes.push(node("ask", "action.reject", json!({})));
        assert_eq!(codes(&g), [DuplicateNodeId]);

        let mut g = valid();
        g.nodes.push(node(" ", "action.reject", json!({})));
        assert_eq!(codes(&g), [InvalidNodeId]);
    }

    #[test]
    fn invalid_params() {
        let cases = [
            ("t", json!({"symbol": "BTCUSD", "threshold_bps": 200})), // missing window_secs
            (
                "t",
                json!({"symbol": "", "threshold_bps": 200, "window_secs": 60}),
            ),
            (
                "t",
                json!({"symbol": "BTCUSD", "threshold_bps": 0, "window_secs": 60}),
            ),
            (
                "t",
                json!({"symbol": "BTCUSD", "threshold_bps": 200, "window_secs": 60, "x": 1}),
            ),
            ("conv", json!({"min_bps": 10001})),
            ("conv", json!({"min_bps": "6000"})),
            ("conv", json!({"min_bps": 60.5})),
            ("conv", json!({})),
            ("conv", json!(null)),
            ("conv", json!([6000])),
            ("stake", json!({"max": 5000000})), // money must be a string
            ("stake", json!({"max": "5.0"})),
            ("stake", json!({"max": "-5"})),
            ("stake", json!({"max": "05"})),
            ("propose", json!({"anything": true})),
        ];
        for (id, params) in cases {
            let mut g = valid();
            let n = g.nodes.iter_mut().find(|n| n.id == id).unwrap();
            n.params = params.clone();
            let errors = validate(&g);
            assert!(
                !errors.is_empty() && errors.iter().all(|e| e.code == InvalidParams),
                "{id} {params}: {errors:?}"
            );
            assert_eq!(errors[0].node_id.as_deref(), Some(id));
        }
    }

    #[test]
    fn params_boundaries_are_accepted() {
        let mut g = valid();
        g.nodes[2].params = json!({"min_bps": 0});
        assert_eq!(validate(&g), vec![]);
        g.nodes[2].params = json!({"min_bps": 10000});
        assert_eq!(validate(&g), vec![]);
        g.nodes[4].params = json!({"max": "0"});
        assert_eq!(validate(&g), vec![]);
    }

    #[test]
    fn no_trigger() {
        let mut g = valid();
        g.nodes.remove(0);
        g.edges.remove(0);
        assert_eq!(codes(&g), [NoTrigger]);
    }

    #[test]
    fn multiple_triggers() {
        let mut g = valid();
        g.nodes
            .push(node("t2", "trigger.schedule", json!({"every_secs": 60})));
        g.edges.push(edge("t2", "out", "eval"));
        let e = find(&g, MultipleTriggers);
        assert_eq!(e.node_id.as_deref(), Some("t2"));
        assert_eq!(codes(&g), [MultipleTriggers]);
    }

    #[test]
    fn edge_to_unknown_node() {
        let mut g = valid();
        g.edges.push(edge("conv", "false", "ghost"));
        let e = find(&g, UnknownEdgeNode);
        assert_eq!(e.node_id.as_deref(), Some("conv"));
        assert_eq!(codes(&g), [UnknownEdgeNode]);

        let mut g = valid();
        g.edges.push(edge("ghost", "out", "phantom"));
        assert_eq!(codes(&g), [UnknownEdgeNode, UnknownEdgeNode]);
    }

    #[test]
    fn unknown_ports() {
        let mut g = valid();
        g.edges[1].from_port = "result".into();
        let e = find(&g, UnknownPort);
        assert_eq!(e.node_id.as_deref(), Some("eval"));
        assert_eq!(codes(&g), [UnknownPort]);

        let mut g = valid();
        g.edges[1].to_port = "input".into();
        let e = find(&g, UnknownPort);
        assert_eq!(e.node_id.as_deref(), Some("conv"));
    }

    #[test]
    fn nothing_can_feed_a_trigger_or_leave_an_action() {
        let mut g = valid();
        g.nodes.push(node("rej", "action.reject", json!({})));
        g.edges.push(edge("conv", "false", "rej"));
        assert_eq!(validate(&g), vec![]);
        // An action has no output ports.
        g.nodes.push(node("rej2", "action.reject", json!({})));
        g.edges.push(edge("rej", "out", "rej2"));
        assert_eq!(codes(&g), [UnknownPort]);
    }

    #[test]
    fn incompatible_port_types() {
        // Event straight into a block that expects an assessment.
        let mut g = valid();
        g.edges.push(edge("t", "out", "conv"));
        assert_eq!(codes(&g), [IncompatiblePorts]);
        assert_eq!(find(&g, IncompatiblePorts).node_id.as_deref(), Some("conv"));

        // An assessment cannot be proposed without generating terms first.
        let mut g = valid();
        g.edges.push(edge("conv", "false", "propose"));
        assert_eq!(codes(&g), [IncompatiblePorts]);
    }

    #[test]
    fn any_input_accepts_every_type() {
        let mut g = valid();
        g.nodes.push(node("rej", "action.reject", json!({})));
        g.edges.push(edge("t", "out", "rej")); // event
        g.edges.push(edge("conv", "false", "rej")); // assessment
        g.edges.push(edge("gen", "out", "rej")); // proposal
        assert_eq!(validate(&g), vec![]);
    }

    #[test]
    fn cycles_are_rejected() {
        // conv -> gen -> stake -> (false) -> back into conv is type-incompatible,
        // so build a type-correct loop instead: conv.false -> strat -> conv.
        let mut g = valid();
        g.nodes
            .push(node("strat", "cond.matches_strategy", json!({})));
        g.edges.push(edge("conv", "false", "strat"));
        g.edges.push(edge("strat", "true", "conv"));
        assert_eq!(codes(&g), [Cycle]);
    }

    #[test]
    fn self_loop_is_a_cycle() {
        let mut g = valid();
        g.edges.push(edge("conv", "false", "conv"));
        assert_eq!(codes(&g), [Cycle]);
        assert_eq!(find(&g, Cycle).node_id.as_deref(), Some("conv"));
    }

    #[test]
    fn unreachable_nodes() {
        let mut g = valid();
        g.nodes.push(node("orphan", "action.reject", json!({})));
        let e = find(&g, UnreachableNode);
        assert_eq!(e.node_id.as_deref(), Some("orphan"));
        assert_eq!(codes(&g), [UnreachableNode]);

        // A detached island that is internally well-formed is still unreachable.
        let mut g = valid();
        g.nodes
            .push(node("strat", "cond.matches_strategy", json!({})));
        g.nodes.push(node("acc", "action.accept", json!({})));
        g.edges.push(edge("strat", "true", "acc"));
        assert_eq!(codes(&g), [UnreachableNode, UnreachableNode]);
    }

    #[test]
    fn every_path_must_end_in_an_action() {
        // Drop the action behind `stake.true` and `stake.false`.
        let mut g = valid();
        g.nodes.retain(|n| n.id != "propose" && n.id != "ask");
        g.edges.retain(|e| e.from != "stake");
        let e = find(&g, DeadEnd);
        assert_eq!(e.node_id.as_deref(), Some("stake"));
        assert_eq!(codes(&g), [DeadEnd]);

        // A lone trigger is a dead end too.
        let g = Graph {
            schema_version: 1,
            nodes: vec![node("t", "trigger.schedule", json!({"every_secs": 60}))],
            edges: vec![],
        };
        assert_eq!(codes(&g), [DeadEnd]);
    }

    #[test]
    fn an_unconnected_branch_is_allowed() {
        // `conv.false` is not wired in `valid()`: that branch simply stops.
        assert!(
            valid()
                .edges
                .iter()
                .all(|e| e.from_port != "false" || e.from != "conv")
        );
        assert_eq!(validate(&valid()), vec![]);
    }

    #[test]
    fn multiple_problems_are_all_reported() {
        let mut g = valid();
        g.nodes[2].params = json!({"min_bps": -1});
        g.nodes.push(node("orphan", "nope.nope", json!({})));
        g.edges.push(edge("stake", "maybe", "ask"));
        let got = codes(&g);
        for code in [InvalidParams, UnknownNodeType, UnknownPort, UnreachableNode] {
            assert!(got.contains(&code), "{code:?} missing from {got:?}");
        }
    }

    #[test]
    fn graph_error_json_shape() {
        let e = find(
            &Graph {
                schema_version: 1,
                nodes: vec![],
                edges: vec![],
            },
            NoTrigger,
        );
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            json!({"code": "no_trigger", "message": e.message, "node_id": null})
        );
    }

    #[test]
    fn graph_schema_is_exported() {
        let schema = graph_json_schema();
        assert!(schema["properties"]["nodes"].is_object());
        assert!(schema["properties"]["schema_version"].is_object());
    }
}
