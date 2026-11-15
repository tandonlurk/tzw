// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {ChallengeEscrow} from "../src/ChallengeEscrow.sol";
import {MockUSDC} from "./mocks/MockTokens.sol";

/// @dev Drives the escrow with random but mostly-valid actions and tracks token flows independently
///      of the escrow's own accounting (ghost values come from actor balance changes).
contract EscrowHandler is Test {
    ChallengeEscrow public immutable escrow;
    MockUSDC public immutable usdc;
    address public immutable resolver;

    address[] public actors;
    bytes32[] public ids;

    uint256 public ghostDeposited; // tokens that left actors for the escrow
    uint256 public ghostPaidOut; // tokens that left the escrow for actors
    mapping(address => uint256) public ghostNetIn; // per actor: deposited - received

    uint256 public callsCreate;
    uint256 public callsDeposit;
    uint256 public callsResolve;
    uint256 public callsVoid;
    uint256 public callsRefund;
    uint256 public callsClaim;
    uint256 public callsDeferred;

    constructor(ChallengeEscrow escrow_, MockUSDC usdc_, address resolver_) {
        escrow = escrow_;
        usdc = usdc_;
        resolver = resolver_;
        for (uint256 i = 0; i < 4; i++) {
            address a = makeAddr(string.concat("actor", vm.toString(i)));
            actors.push(a);
            usdc.mint(a, 1_000_000e6);
            vm.prank(a);
            usdc.approve(address(escrow), type(uint256).max);
        }
    }

    function idsLength() external view returns (uint256) {
        return ids.length;
    }

    function actorsLength() external view returns (uint256) {
        return actors.length;
    }

    modifier tracked() {
        uint256 n = actors.length;
        uint256[] memory before = new uint256[](n);
        for (uint256 i = 0; i < n; i++) {
            before[i] = usdc.balanceOf(actors[i]);
        }
        _;
        for (uint256 i = 0; i < n; i++) {
            uint256 nowBal = usdc.balanceOf(actors[i]);
            if (nowBal < before[i]) {
                ghostDeposited += before[i] - nowBal;
            } else {
                ghostPaidOut += nowBal - before[i];
            }
        }
    }

    function create(uint256 aSeed, uint256 bSeed, uint256 stake, uint64 fundIn, uint64 resolveIn) public {
        if (ids.length >= 40) return;
        uint256 ai = aSeed % actors.length;
        uint256 bi = bSeed % actors.length;
        if (ai == bi) bi = (bi + 1) % actors.length;
        address a = actors[ai];
        address b = actors[bi];
        stake = bound(stake, 1, 10_000e6);
        uint64 fd = uint64(block.timestamp + bound(fundIn, 1, 2 days));
        uint64 rd = fd + uint64(bound(resolveIn, 1, 7 days));
        bytes32 id = keccak256(abi.encode(ids.length));

        vm.prank(resolver);
        escrow.create(id, a, b, stake, fd, rd);
        ids.push(id);
        callsCreate++;
    }

    /// @dev Biased towards recent challenges, whose funding window is more likely still open.
    function _recentId(uint256 idSeed) internal view returns (bytes32) {
        uint256 window = ids.length < 3 ? ids.length : 3;
        return ids[ids.length - 1 - (idSeed % window)];
    }

    function deposit(uint256 idSeed, bool asA) external tracked {
        if (ids.length == 0) return;
        bytes32 id = _recentId(idSeed);
        ChallengeEscrow.Challenge memory c = escrow.getChallenge(id);
        vm.prank(asA ? c.partyA : c.partyB);
        try escrow.deposit(id) {
            callsDeposit++;
        } catch {}
    }

    /// @dev Create and immediately try to fund from both sides (fails for a currently blocked party).
    function createFunded(uint256 aSeed, uint256 bSeed, uint256 stake, uint64 fundIn, uint64 resolveIn)
        external
        tracked
    {
        uint256 before = ids.length;
        create(aSeed, bSeed, stake, fundIn, resolveIn);
        if (ids.length == before) return;
        bytes32 id = ids[before];
        ChallengeEscrow.Challenge memory c = escrow.getChallenge(id);
        vm.prank(c.partyA);
        try escrow.deposit(id) {
            callsDeposit++;
        } catch {}
        vm.prank(c.partyB);
        try escrow.deposit(id) {
            callsDeposit++;
        } catch {}
    }

    /// @dev Resolve in favour of a party that is blocklisted at that moment, forcing a deferred payout.
    function resolveToBlockedWinner(uint256 idSeed, bool aWins, bool unblockAfter) external tracked {
        if (ids.length == 0) return;
        bytes32 id = _preferActive(idSeed);
        ChallengeEscrow.Challenge memory c = escrow.getChallenge(id);
        address winner = aWins ? c.partyA : c.partyB;
        usdc.setBlocked(winner, true);
        vm.prank(resolver);
        try escrow.resolve(id, winner) {
            callsResolve++;
            callsDeferred++;
        } catch {}
        if (unblockAfter) usdc.setBlocked(winner, false);
    }

    function depositBoth(uint256 idSeed) external tracked {
        if (ids.length == 0) return;
        bytes32 id = _recentId(idSeed);
        ChallengeEscrow.Challenge memory c = escrow.getChallenge(id);
        vm.prank(c.partyA);
        try escrow.deposit(id) {
            callsDeposit++;
        } catch {}
        vm.prank(c.partyB);
        try escrow.deposit(id) {
            callsDeposit++;
        } catch {}
    }

    /// @dev First Active challenge scanning from a random offset, else a random one (which will revert).
    function _preferActive(uint256 idSeed) internal view returns (bytes32) {
        uint256 n = ids.length;
        for (uint256 i = 0; i < n; i++) {
            bytes32 id = ids[(idSeed % n + i) % n];
            if (escrow.getChallenge(id).status == ChallengeEscrow.Status.Active) return id;
        }
        return ids[idSeed % n];
    }

    function resolve(uint256 idSeed, bool aWins) external tracked {
        if (ids.length == 0) return;
        bytes32 id = _preferActive(idSeed);
        ChallengeEscrow.Challenge memory c = escrow.getChallenge(id);
        vm.prank(resolver);
        try escrow.resolve(id, aWins ? c.partyA : c.partyB) {
            callsResolve++;
        } catch {}
    }

    /// @dev Only acts one time in three so that most challenges live long enough to be funded.
    function resolveVoid(uint256 idSeed, uint8 gate) external tracked {
        if (ids.length == 0 || gate % 3 != 0) return;
        vm.prank(resolver);
        try escrow.resolveVoid(ids[idSeed % ids.length]) {
            callsVoid++;
        } catch {}
    }

    function refund(uint256 idSeed) external tracked {
        if (ids.length == 0) return;
        try escrow.refund(ids[idSeed % ids.length]) {
            callsRefund++;
        } catch {}
    }

    /// @dev Tries every deferred payout; ones whose recipient is still blocked stay owed.
    function claimAll() external tracked {
        for (uint256 i = 0; i < ids.length; i++) {
            for (uint256 j = 0; j < actors.length; j++) {
                if (escrow.owed(ids[i], actors[j]) == 0) continue;
                try escrow.claim(ids[i], actors[j]) {
                    callsClaim++;
                } catch {}
            }
        }
    }

    function claim(uint256 idSeed, uint256 actorSeed) external tracked {
        if (ids.length == 0) return;
        try escrow.claim(ids[idSeed % ids.length], actors[actorSeed % actors.length]) {
            callsClaim++;
        } catch {}
    }

    /// @dev Blocks one time in four, otherwise unblocks, so most actors can transact most of the time.
    function setBlocked(uint256 actorSeed, uint8 mode) external {
        usdc.setBlocked(actors[actorSeed % actors.length], mode % 4 == 0);
    }

    function warp(uint32 by, bool long) external {
        vm.warp(block.timestamp + (long ? bound(by, 1 hours, 3 days) : bound(by, 1, 20 minutes)));
    }
}

contract ChallengeEscrowInvariantTest is Test {
    MockUSDC usdc;
    ChallengeEscrow escrow;
    EscrowHandler handler;
    address resolver = makeAddr("resolver");

    function setUp() public {
        vm.warp(1_800_000_000);
        usdc = new MockUSDC();
        escrow = new ChallengeEscrow(address(usdc), resolver);
        handler = new EscrowHandler(escrow, usdc, resolver);
        targetContract(address(handler));
    }

    /// @dev What the escrow should be holding, recomputed from per-challenge state.
    function _outstanding() internal view returns (uint256 total) {
        uint256 n = handler.idsLength();
        uint256 m = handler.actorsLength();
        for (uint256 i = 0; i < n; i++) {
            bytes32 id = handler.ids(i);
            ChallengeEscrow.Challenge memory c = escrow.getChallenge(id);
            if (c.status == ChallengeEscrow.Status.Created || c.status == ChallengeEscrow.Status.Active) {
                if (c.depositedA) total += c.stake;
                if (c.depositedB) total += c.stake;
            }
            for (uint256 j = 0; j < m; j++) {
                total += escrow.owed(id, handler.actors(j));
            }
        }
    }

    /// Token balance == sum of outstanding deposits (locked stakes + deferred payouts) == totalHeld.
    function invariant_balanceEqualsOutstandingDeposits() public view {
        uint256 balance = usdc.balanceOf(address(escrow));
        assertEq(balance, _outstanding(), "balance != outstanding");
        assertEq(balance, escrow.totalHeld(), "balance != totalHeld");
    }

    /// Total paid out never exceeds total deposited, and the difference is exactly what is held.
    function invariant_paidOutNeverExceedsDeposited() public view {
        assertLe(handler.ghostPaidOut(), handler.ghostDeposited(), "paid out more than deposited");
        assertEq(handler.ghostDeposited() - handler.ghostPaidOut(), usdc.balanceOf(address(escrow)), "flow mismatch");
    }

    /// Active means both deposited; a winner is set only when Resolved and is always a party.
    function invariant_statusConsistency() public view {
        uint256 n = handler.idsLength();
        for (uint256 i = 0; i < n; i++) {
            ChallengeEscrow.Challenge memory c = escrow.getChallenge(handler.ids(i));
            assertTrue(c.status != ChallengeEscrow.Status.None);
            if (c.status == ChallengeEscrow.Status.Active || c.status == ChallengeEscrow.Status.Resolved) {
                assertTrue(c.depositedA && c.depositedB, "active/resolved without both deposits");
            }
            if (c.status == ChallengeEscrow.Status.Created) {
                assertFalse(c.depositedA && c.depositedB, "fully funded but not active");
            }
            if (c.status == ChallengeEscrow.Status.Resolved) {
                assertTrue(c.winner == c.partyA || c.winner == c.partyB, "winner not a party");
            } else {
                assertEq(c.winner, address(0), "winner set while unresolved");
            }
        }
    }

    /// The escrow never holds or needs native value.
    function invariant_noNativeBalance() public view {
        assertEq(address(escrow).balance, 0);
    }

    /// Not an assertion: run with -vv to confirm the handler reaches every path.
    function afterInvariant() public {
        emit log_named_uint("create", handler.callsCreate());
        emit log_named_uint("deposit", handler.callsDeposit());
        emit log_named_uint("resolve", handler.callsResolve());
        emit log_named_uint("resolveVoid", handler.callsVoid());
        emit log_named_uint("refund", handler.callsRefund());
        emit log_named_uint("deferred", handler.callsDeferred());
        emit log_named_uint("claim", handler.callsClaim());
    }
}
