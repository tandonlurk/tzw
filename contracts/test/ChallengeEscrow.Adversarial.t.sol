// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {ChallengeEscrow} from "../src/ChallengeEscrow.sol";
import {MockUSDC} from "./mocks/MockTokens.sol";

/// @dev Adversarial tests covering gaps in ChallengeEscrow.t.sol / .invariant.t.sol:
///      donations, resolver-as-party, unbounded deadlines, extreme stakes, claim redirection,
///      and "no dust left behind" over random lifecycles.
contract ChallengeEscrowAdversarialTest is Test {
    MockUSDC usdc;
    ChallengeEscrow escrow;

    address resolver = makeAddr("resolver");
    address alice = makeAddr("alice");
    address bob = makeAddr("bob");
    address mallory = makeAddr("mallory");

    bytes32 constant ID = keccak256("adv-1");
    uint256 constant BAL = 1_000_000e6;

    function setUp() public {
        vm.warp(1_800_000_000);
        usdc = new MockUSDC();
        escrow = new ChallengeEscrow(address(usdc), resolver);
        _fund(alice);
        _fund(bob);
        _fund(mallory);
    }

    function _fund(address who) internal {
        usdc.mint(who, BAL);
        vm.prank(who);
        usdc.approve(address(escrow), type(uint256).max);
    }

    function _create(bytes32 id, address a, address b, uint256 stake, uint64 fd, uint64 rd) internal {
        vm.prank(resolver);
        escrow.create(id, a, b, stake, fd, rd);
    }

    function _activate(bytes32 id, uint256 stake) internal {
        _create(id, alice, bob, stake, uint64(block.timestamp + 1 hours), uint64(block.timestamp + 1 days));
        vm.prank(alice);
        escrow.deposit(id);
        vm.prank(bob);
        escrow.deposit(id);
    }

    /// Anyone can send tokens straight to the escrow. This breaks the README claim that the token balance
    /// "equals totalHeld" (it is >=), but must not break solvency or later accounting.
    function testFuzz_donation_doesNotBreakAccounting(uint96 donation, uint64 stake, bool aWins) public {
        stake = uint64(bound(stake, 1, 100_000e6));
        donation = uint96(bound(donation, 0, BAL));
        vm.prank(mallory);
        usdc.transfer(address(escrow), donation);

        _activate(ID, stake);
        assertGe(usdc.balanceOf(address(escrow)), escrow.totalHeld());
        assertEq(escrow.totalHeld(), uint256(stake) * 2);

        vm.prank(resolver);
        escrow.resolve(ID, aWins ? alice : bob);

        assertEq(escrow.totalHeld(), 0);
        // the donation is stranded: there is no sweep (documented as SEC-06, informational)
        assertEq(usdc.balanceOf(address(escrow)), donation);
        assertEq(usdc.balanceOf(aWins ? alice : bob), BAL + stake);
    }

    /// The resolver can also be a party and then decide its own challenge. This is inherent in the
    /// resolver-trust model, but the contract does not prevent a conflicted resolver.
    function test_resolverCanBeAPartyAndResolveInItsOwnFavour() public {
        _create(ID, resolver, bob, 10e6, uint64(block.timestamp + 1 hours), uint64(block.timestamp + 1 days));
        usdc.mint(resolver, 10e6);
        vm.prank(resolver);
        usdc.approve(address(escrow), type(uint256).max);
        vm.prank(resolver);
        escrow.deposit(ID);
        vm.prank(bob);
        escrow.deposit(ID);
        vm.prank(resolver);
        escrow.resolve(ID, resolver);
        assertEq(usdc.balanceOf(resolver), 20e6);
    }

    /// There is no upper bound on resolveDeadline. A resolver (or a bug in the backend) can create a
    /// challenge whose timeout refund is effectively never available; the stakes are then only
    /// released by the resolver. Parties must verify getChallenge() terms before depositing.
    function test_unboundedResolveDeadline_removesTheTimeoutRefund() public {
        _create(ID, alice, bob, 10e6, uint64(block.timestamp + 1 hours), type(uint64).max);
        vm.prank(alice);
        escrow.deposit(ID);
        vm.prank(bob);
        escrow.deposit(ID);

        vm.warp(block.timestamp + 365 days * 100);
        vm.expectRevert(ChallengeEscrow.RefundNotAvailable.selector);
        escrow.refund(ID);

        // only the resolver can still release the funds
        vm.prank(resolver);
        escrow.resolveVoid(ID);
        assertEq(usdc.balanceOf(alice), BAL);
    }

    /// A stake so large that two deposits overflow totalHeld can never activate, and the single
    /// depositor is always able to get their money back after the funding deadline.
    function testFuzz_hugeStakeNeverActivatesAndDepositorIsRefunded(uint256 stake) public {
        stake = bound(stake, 1 << 255, type(uint256).max);
        deal(address(usdc), alice, stake); // set balance directly: minting would overflow totalSupply
        deal(address(usdc), bob, stake);
        uint256 aliceBefore = usdc.balanceOf(alice);
        _create(ID, alice, bob, stake, uint64(block.timestamp + 1 hours), uint64(block.timestamp + 1 days));

        vm.prank(alice);
        escrow.deposit(ID);
        vm.prank(bob);
        vm.expectRevert(); // arithmetic overflow on totalHeld
        escrow.deposit(ID);

        vm.warp(block.timestamp + 1 hours);
        escrow.refund(ID);
        assertEq(usdc.balanceOf(alice), aliceBefore);
        assertEq(escrow.totalHeld(), 0);
    }

    /// claim() must pay only the named party, whoever calls it.
    function testFuzz_claimCannotRedirectFunds(address caller) public {
        vm.assume(caller != address(escrow) && caller != address(0) && caller != alice && caller != bob);
        _activate(ID, 10e6);
        usdc.setBlocked(bob, true);
        vm.prank(resolver);
        escrow.resolve(ID, bob); // payout deferred
        assertEq(escrow.owed(ID, bob), 20e6);
        usdc.setBlocked(bob, false);

        uint256 callerBefore = usdc.balanceOf(caller);
        uint256 aliceBefore = usdc.balanceOf(alice);
        vm.prank(caller);
        escrow.claim(ID, bob);
        assertEq(usdc.balanceOf(caller), callerBefore);
        assertEq(usdc.balanceOf(alice), aliceBefore);
        assertEq(usdc.balanceOf(bob), BAL - 10e6 + 20e6);
        // a caller can't claim for a non-owed party
        vm.prank(caller);
        vm.expectRevert(ChallengeEscrow.NothingOwed.selector);
        escrow.claim(ID, caller);
    }

    /// Random lifecycles, including blocked parties and late unblocking: when everything is
    /// terminal and claimed, the escrow holds nothing and owes nothing.
    function testFuzz_noDustAfterAnyTerminalPath(uint64 stake, uint8 path, bool blockBob) public {
        stake = uint64(bound(stake, 1, 100_000e6));
        path = uint8(bound(path, 0, 3));
        _activate(ID, stake);
        if (blockBob) usdc.setBlocked(bob, true);

        if (path == 0) {
            vm.prank(resolver);
            escrow.resolve(ID, alice);
        } else if (path == 1) {
            vm.prank(resolver);
            escrow.resolve(ID, bob);
        } else if (path == 2) {
            vm.prank(resolver);
            escrow.resolveVoid(ID);
        } else {
            vm.warp(block.timestamp + 1 days + escrow.GRACE());
            escrow.refund(ID);
        }
        usdc.setBlocked(bob, false);
        if (escrow.owed(ID, bob) != 0) escrow.claim(ID, bob);
        if (escrow.owed(ID, alice) != 0) escrow.claim(ID, alice);

        assertEq(escrow.totalHeld(), 0);
        assertEq(usdc.balanceOf(address(escrow)), 0);
        assertEq(usdc.balanceOf(alice) + usdc.balanceOf(bob), 2 * BAL);
    }

    /// Terminal states are absorbing: after any terminal path no entrypoint can move funds again.
    function testFuzz_terminalStateIsAbsorbing(uint8 path, uint8 attempt) public {
        path = uint8(bound(path, 0, 2));
        attempt = uint8(bound(attempt, 0, 4));
        _activate(ID, 10e6);
        vm.startPrank(resolver);
        if (path == 0) escrow.resolve(ID, alice);
        else if (path == 1) escrow.resolveVoid(ID);
        vm.stopPrank();
        if (path == 2) {
            vm.warp(block.timestamp + 2 days);
            escrow.refund(ID);
        }
        uint256 before = usdc.balanceOf(alice) + usdc.balanceOf(bob);
        vm.warp(block.timestamp + 365 days);
        vm.startPrank(resolver);
        if (attempt == 0) vm.expectRevert();
        if (attempt == 0) escrow.resolve(ID, bob);
        if (attempt == 1) vm.expectRevert();
        if (attempt == 1) escrow.resolveVoid(ID);
        vm.stopPrank();
        if (attempt == 2) {
            vm.expectRevert();
            escrow.refund(ID);
        }
        if (attempt == 3) {
            vm.prank(alice);
            vm.expectRevert();
            escrow.deposit(ID);
        }
        if (attempt == 4) {
            vm.expectRevert();
            vm.prank(resolver);
            escrow.create(ID, alice, bob, 1, uint64(block.timestamp + 1), uint64(block.timestamp + 2));
        }
        assertEq(usdc.balanceOf(alice) + usdc.balanceOf(bob), before);
        assertEq(escrow.totalHeld(), 0);
    }

    /// Many concurrent challenges sharing parties: a token block on one party in one challenge cannot
    /// reduce what the other challenges pay.
    function testFuzz_manyChallengesAreIsolated(uint8 n, uint64 stakeSeed) public {
        n = uint8(bound(n, 2, 12));
        for (uint256 i = 0; i < n; i++) {
            _activate(keccak256(abi.encode("c", i)), 1e6 + (uint256(stakeSeed) % 1000) * (i + 1));
        }
        usdc.setBlocked(bob, true);
        for (uint256 i = 0; i < n; i++) {
            vm.prank(resolver);
            escrow.resolve(keccak256(abi.encode("c", i)), i % 2 == 0 ? alice : bob);
        }
        usdc.setBlocked(bob, false);
        uint256 owedTotal;
        for (uint256 i = 0; i < n; i++) {
            bytes32 id = keccak256(abi.encode("c", i));
            uint256 o = escrow.owed(id, bob);
            owedTotal += o;
            if (i % 2 == 1) assertGt(o, 0);
            else assertEq(o, 0);
            if (o != 0) escrow.claim(id, bob);
        }
        assertEq(escrow.totalHeld(), 0);
        assertEq(usdc.balanceOf(address(escrow)), 0);
        assertGt(owedTotal, 0);
    }
}
