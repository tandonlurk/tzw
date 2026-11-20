#!/usr/bin/env python3
"""Populate a fresh fake-market server via public APIs. All funds are simulated."""
import copy
import json
import os
import sys
from datetime import datetime, timedelta, timezone
from urllib.error import HTTPError, URLError
from urllib.parse import urlparse
from urllib.request import Request, build_opener, HTTPRedirectHandler

UNIT = 1_000_000
TOTAL = 6_187_430_000
MARKER = "video-portfolio-demo:v2"
# Every amount is an integer USDC minor-unit value. Clone budgets divide
# exactly, leaving 1,576.75 USDC unallocated without rounding drift.
ROOTS = [
    {"name": "Atlas Momentum", "template": "Momentum bull", "clones": 3,
     "maximum": 6_170_000, "budget": 1_327_190_000, "root_allocation": 317_840_000,
     "description": "Momentum-led strategy with bounded exposure."},
    {"name": "Vega Contrarian", "template": "Skeptic", "clones": 17,
     "maximum": 7_830_000, "budget": 1_739_410_000, "root_allocation": 283_530_000,
     "description": "Contrarian strategy focused on incoming opportunities."},
    {"name": "Lumen Cautious", "template": "Cautious", "clones": 67,
     "maximum": 3_910_000, "budget": 1_544_080_000, "root_allocation": 241_600_000,
     "description": "Selective strategy with smaller commitments."},
]
DEPOSITS = [amount * 10_000 for amount in (
    48_317, 52_743, 61_108, 39_861, 75_429, 46_283,
    53_917, 31_789, 68_541, 42_876, 57_192, 40_687,
)]
assert sum(DEPOSITS) == TOTAL
assert TOTAL - sum(root["budget"] for root in ROOTS) == 1_576_750_000
assert all((root["budget"] - root["root_allocation"]) % root["clones"] == 0 for root in ROOTS)


class NoRedirect(HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        # Never forward a credential to a redirected origin.
        return None


class Api:
    def __init__(self):
        self.base = os.environ.get("DEMO_API_URL", "http://127.0.0.1:8091/api").rstrip("/")
        parsed = urlparse(self.base)
        if parsed.scheme not in ("http", "https") or parsed.username or parsed.password:
            raise RuntimeError("DEMO_API_URL must be an HTTP(S) URL without credentials")
        if parsed.scheme == "http" and parsed.hostname not in ("localhost", "127.0.0.1", "::1"):
            raise RuntimeError("Nonlocal demo endpoints must use HTTPS")
        self.token = os.environ.get("AEN_API_TOKEN", "")
        self.client = build_opener(NoRedirect)

    def call(self, method, path, body=None, key=None):
        headers = {"Accept": "application/json"}
        if self.token:
            headers["Authorization"] = "Bearer " + self.token
        payload = None
        if body is not None:
            headers["Content-Type"] = "application/json"
            payload = json.dumps(body).encode()
        if key:
            headers["Idempotency-Key"] = key
        req = Request(self.base + path, data=payload, headers=headers, method=method)
        try:
            with self.client.open(req, timeout=60) as response:
                return json.load(response)
        except HTTPError as error:
            try:
                problem = json.load(error).get("error", {})
                message = problem.get("message", "request rejected")
            except (ValueError, AttributeError):
                message = "request rejected"
            raise RuntimeError(f"{method} {path}: HTTP {error.code}: {message}") from None
        except URLError:
            raise RuntimeError(f"Cannot reach demo API for {method} {path}") from None

    def get(self, path):
        return self.call("GET", path)

    def post(self, path, body, key=None):
        return self.call("POST", path, body, key)


def stamp(when):
    return when.isoformat(timespec="seconds").replace("+00:00", "Z")


def parse_stamp(value):
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


def agent_payload(template, root):
    value = {key: copy.deepcopy(template[key]) for key in ("config", "policy", "graph")}
    maximum = root["maximum"]
    value.update(name=root["name"], description=f"{root['description']} Simulated. {MARKER}")
    value["config"].update(horizon_secs=300, schedule_secs=60)
    value["policy"].update(
        max_per_interaction=str(maximum),
        max_aggregate_exposure=str(250 * UNIT),
        approval_threshold=str(maximum),
        min_conviction_bps=5000,
        stop_loss=str(400 * UNIT),
    )
    for node in value["graph"]["nodes"]:
        kind = node["type"]
        if kind == "cond.stake_lte":
            node["params"]["max"] = str(maximum)
        elif kind == "cond.exposure_within":
            node["params"]["max"] = str(100 * UNIT)
        elif kind == "cond.conviction_gte":
            node["params"]["min_bps"] = 6000
        elif kind == "trigger.schedule":
            node["params"]["every_secs"] = 60
        elif kind == "action.request_approval":
            node["type"] = "action.propose_challenge"
            node["params"] = {}
    return value


def tick(api, when):
    report = api.post("/dev/tick", {"now": stamp(when)})
    if report.get("errors"):
        raise RuntimeError("Lifecycle tick failed: " + "; ".join(report["errors"]))
    return report


def open_interactions(api):
    return [item for item in api.get("/interactions?limit=500")
            if item["state"] in ("proposed", "negotiating", "accepted", "awaiting_approval", "funded", "active", "resolving")]


def start_round(api, when):
    api.post("/dev/fake-market", {"move_bps": 500, "yes_price_bps": 5500, "resolution_side": "open", "fail": False})
    for step in range(4):
        tick(api, when + timedelta(seconds=step * 5))
        opened = open_interactions(api)
        if opened and all(item["state"] == "active" for item in opened):
            return opened
    raise RuntimeError("The demo agents did not reach active challenges automatically; no history was fabricated")


def summary(api):
    wallet = api.get("/wallet")
    agents = api.get("/agents")
    interactions = api.get("/interactions?limit=500")
    settled = [item for item in interactions if item["state"] == "settled"]
    if wallet["env"] != "simulated" or any(agent["env"] != "simulated" for agent in agents):
        raise RuntimeError("This seed only supports simulated agents and funds")
    total = int(wallet["unallocated"]) + int(wallet["allocated"])
    if total != TOTAL:
        raise RuntimeError(f"Wallet conservation check failed: expected {TOTAL} minor units, got {total}")
    families = []
    expected_count = sum(root["clones"] + 1 for root in ROOTS)
    if len(agents) != expected_count or len([agent for agent in agents if not agent.get("clone")]) != 3:
        raise RuntimeError("Expected exactly three roots and 90 agents including clones")
    for spec in ROOTS:
        name, instances = spec["name"], spec["clones"] + 1
        root = next((agent for agent in agents if agent["name"] == name and not agent.get("clone")), None)
        if root is None:
            raise RuntimeError("Missing expected demo strategy root")
        members = [agent for agent in agents if agent.get("family_id") == root["id"]]
        if len(members) != instances:
            raise RuntimeError(f"Unexpected family size for {name}: {len(members)} instead of {instances}")
        families.append({"name": name, "root_id": root["id"], "clones": spec["clones"], "instances": instances,
                         "family_balance_usdc": sum(int(a["balances"]["available"]) + int(a["balances"]["committed"]) for a in members) / UNIT})
    ledger_entries = sum(len(api.get(f"/agents/{a['id']}/ledger")) for a in agents)
    return {"env": "simulated", "total_usdc": total / UNIT,
            "unallocated_usdc": int(wallet["unallocated"]) / UNIT,
            "committed_usdc": int(wallet["committed"]) / UNIT,
            "wallet_entries": len(wallet["entries"]), "agent_ledger_entries": ledger_entries,
            "settled_challenges": len(settled), "open_challenges": len(open_interactions(api)),
            "strategy_roots": families, "note": "Synthetic demo scenario, not real trading or investment performance."}


def main():
    api = Api()
    existing = api.get("/agents")
    wallet = api.get("/wallet")
    if existing:
        if all(MARKER in agent.get("description", "") for agent in existing):
            result = summary(api)
            if result["settled_challenges"] >= 30:
                print(json.dumps(result, indent=2))
                return
        raise RuntimeError("Refusing a nonempty/partially seeded database. Use a fresh isolated demo database; never reset an existing owner database.")
    if wallet["env"] != "simulated" or any(int(wallet[k]) for k in ("unallocated", "allocated", "committed")):
        raise RuntimeError("Refusing nonempty or nonsimulated wallet")
    if api.get("/interactions?limit=1"):
        raise RuntimeError("Refusing database with preexisting interactions")
    # Refuse non-fake servers before any financial or agent mutation.
    api.post("/dev/fake-market", {"move_bps": 500, "yes_price_bps": 5500, "resolution_side": "open"})
    templates = {item["name"]: item for item in api.get("/templates")}
    roots = []
    for spec in ROOTS:
        root = api.post("/agents", agent_payload(templates[spec["template"]], spec))
        roots.append((root, spec))
    # Twelve distinct funding receipts make a useful wallet timeline. Each is
    # a normal simulated deposit, not a transfer misrepresented as profit.
    for index, amount in enumerate(DEPOSITS):
        api.post("/wallet/simulated-deposit", {"amount": str(amount)}, f"{MARKER}:deposit:{index}")
    for index, (root, spec) in enumerate(roots):
        allocation = spec["root_allocation"]
        api.post(f"/agents/{root['id']}/allocate", {"amount": str(allocation)}, f"{MARKER}:root:{index}")
        api.post(f"/agents/{root['id']}/multiply", {
            "count": spec["clones"], "variance_bps": 1000,
            "budget": str(spec["budget"] - allocation), "seed": 20261120 + index,
        })
    present = datetime.now(timezone.utc).replace(microsecond=0)
    for round_index in range(12):
        when = present - timedelta(hours=48 - round_index * 4)
        opened = start_round(api, when)
        end = max(parse_stamp(item["end_at"]) for item in opened) + timedelta(seconds=5)
        # Vary deterministic fake observations, not hand-picked ledger
        # payouts. Settlement still follows each challenge's formula.
        price = [7000, 3000, 7000, 7000, 3000, 7000][round_index % 6]
        api.post("/dev/fake-market", {"move_bps": 0, "yes_price_bps": price})
        tick(api, end)
        if open_interactions(api):
            raise RuntimeError("Historical challenges did not all settle; refusing to fabricate results")
        print(f"Settled historical batch {round_index + 1}/12", file=sys.stderr)
    start_round(api, datetime.now(timezone.utc).replace(microsecond=0))
    result = summary(api)
    if result["settled_challenges"] < 30:
        raise RuntimeError("Too few actual settlements for the requested demo history")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, KeyError, ValueError) as error:
        print(f"Demo seed stopped: {error}", file=sys.stderr)
        sys.exit(1)
