// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {ChallengeEscrow} from "../src/ChallengeEscrow.sol";
import {MockUSDC, FeeOnTransferToken, FalseReturningToken, LyingToken, ReentrantToken} from "./mocks/MockTokens.sol";

contract ChallengeEscrowTest is Test {
    event Created(
        bytes32 indexed id,
        address indexed partyA,
        address indexed partyB,
        uint256 stake,
        uint64 fundDeadline,
        uint64 resolveDeadline
    );
    event Deposited(bytes32 indexed id, address indexed party, uint256 amount);
    event Activated(bytes32 indexed id, address indexed partyA, address indexed partyB, uint256 totalStaked);
    event Resolved(bytes32 indexed id, address indexed winner, address indexed loser, uint256 payout);
    event Voided(bytes32 indexed id, address indexed partyA, address indexed partyB, uint256 refundA, uint256 refundB);
    event Refunded(
        bytes32 indexed id, address indexed partyA, address indexed partyB, uint256 refundA, uint256 refundB
    );
    event PayoutDeferred(bytes32 indexed id, address indexed party, uint256 amount);
    event Claimed(bytes32 indexed id, address indexed party, uint256 amount);

    MockUSDC usdc;
    ChallengeEscrow escrow;

    address resolver = makeAddr("resolver");
    address alice = makeAddr("alice");
    address bob = makeAddr("bob");
    address carol = makeAddr("carol");

    bytes32 constant ID = keccak256("challenge-1");
    uint256 constant STAKE = 25e6; // 25 USDC
    uint256 constant START_BALANCE = 1_000e6;
    uint64 fundDeadline;
    uint64 resolveDeadline;
    uint64 grace;

    function setUp() public {
        vm.warp(1_800_000_000);
        usdc = new MockUSDC();
        escrow = new ChallengeEscrow(address(usdc), resolver);
        grace = escrow.GRACE();
        fundDeadline = uint64(block.timestamp + 1 hours);
        resolveDeadline = uint64(block.timestamp + 1 days);
        _fund(usdc, escrow, alice);
        _fund(usdc, escrow, bob);
        _fund(usdc, escrow, carol);
    }

    // ---------------------------------------------------------------- helpers

    function _fund(MockUSDC token, ChallengeEscrow e, address who) internal {
        token.mint(who, START_BALANCE);
        vm.prank(who);
        token.approve(address(e), type(uint256).max);
    }

    function _create() internal {
        vm.prank(resolver);
        escrow.create(ID, alice, bob, STAKE, fundDeadline, resolveDeadline);
    }

    function _deposit(address who) internal {
        vm.prank(who);
        escrow.deposit(ID);
    }

    function _activate() internal {
        _create();
        _deposit(alice);
        _deposit(bob);
    }

    function _status(bytes32 id) internal view returns (ChallengeEscrow.Status) {
        return escrow.getChallenge(id).status;
    }

    function _wrongStatus(ChallengeEscrow.Status s) internal pure returns (bytes memory) {
        return abi.encodeWithSelector(ChallengeEscrow.WrongStatus.selector, s);
    }

    function _assertSettledClean() internal view {
        assertEq(usdc.balanceOf(address(escrow)), 0, "escrow balance");
        assertEq(escrow.totalHeld(), 0, "totalHeld");
    }

    // ------------------------------------------------------------ constructor

    function test_constructor_setsImmutables() public view {
        assertEq(escrow.USDC(), address(usdc));
        assertEq(escrow.RESOLVER(), resolver);
    }

    function test_constructor_revertsOnZeroAddress() public {
        vm.expectRevert(ChallengeEscrow.ZeroAddress.selector);
        new ChallengeEscrow(address(0), resolver);
        vm.expectRevert(ChallengeEscrow.ZeroAddress.selector);
        new ChallengeEscrow(address(usdc), address(0));
    }

    function test_rejectsNativeValue() public {
        vm.deal(alice, 1 ether);
        vm.prank(alice);
        (bool ok,) = address(escrow).call{value: 1}("");
        assertFalse(ok, "plain value transfer must fail");
        vm.prank(alice);
        (ok,) = address(escrow).call{value: 1}(abi.encodeCall(ChallengeEscrow.deposit, (ID)));
        assertFalse(ok, "value with call must fail");
        assertEq(address(escrow).balance, 0);
    }

    // ----------------------------------------------------------------- create

    function test_create_storesAndEmits() public {
        vm.expectEmit(address(escrow));
        emit Created(ID, alice, bob, STAKE, fundDeadline, resolveDeadline);
        _create();

        ChallengeEscrow.Challenge memory c = escrow.getChallenge(ID);
        assertEq(c.partyA, alice);
        assertEq(c.partyB, bob);
        assertEq(c.stake, STAKE);
        assertEq(c.fundDeadline, fundDeadline);
        assertEq(c.resolveDeadline, resolveDeadline);
        assertEq(uint8(c.status), uint8(ChallengeEscrow.Status.Created));
        assertFalse(c.depositedA);
        assertFalse(c.depositedB);
        assertEq(c.winner, address(0));
    }

    function test_getChallenge_unknownIdIsNone() public view {
        assertEq(uint8(_status(keccak256("nope"))), uint8(ChallengeEscrow.Status.None));
    }

    function test_create_revertsForNonResolver() public {
        vm.prank(alice);
        vm.expectRevert(ChallengeEscrow.NotResolver.selector);
        escrow.create(ID, alice, bob, STAKE, fundDeadline, resolveDeadline);
    }

    function test_create_revertsOnDuplicateId() public {
        _create();
        vm.prank(resolver);
        vm.expectRevert(ChallengeEscrow.ChallengeExists.selector);
        escrow.create(ID, carol, bob, STAKE, fundDeadline, resolveDeadline);
    }

    function test_create_revertsOnDuplicateIdAfterTerminal() public {
        _create();
        vm.prank(resolver);
        escrow.resolveVoid(ID);
        vm.prank(resolver);
        vm.expectRevert(ChallengeEscrow.ChallengeExists.selector);
        escrow.create(ID, alice, bob, STAKE, fundDeadline, resolveDeadline);
    }

    function test_create_revertsOnZeroStake() public {
        vm.prank(resolver);
        vm.expectRevert(ChallengeEscrow.ZeroStake.selector);
        escrow.create(ID, alice, bob, 0, fundDeadline, resolveDeadline);
    }

    function test_create_revertsOnInvalidParties() public {
        vm.startPrank(resolver);
        vm.expectRevert(ChallengeEscrow.InvalidParties.selector);
        escrow.create(ID, address(0), bob, STAKE, fundDeadline, resolveDeadline);
        vm.expectRevert(ChallengeEscrow.InvalidParties.selector);
        escrow.create(ID, alice, address(0), STAKE, fundDeadline, resolveDeadline);
        vm.expectRevert(ChallengeEscrow.InvalidParties.selector);
        escrow.create(ID, alice, alice, STAKE, fundDeadline, resolveDeadline);
        vm.expectRevert(ChallengeEscrow.InvalidParties.selector);
        escrow.create(ID, address(escrow), bob, STAKE, fundDeadline, resolveDeadline);
        vm.expectRevert(ChallengeEscrow.InvalidParties.selector);
        escrow.create(ID, alice, address(escrow), STAKE, fundDeadline, resolveDeadline);
        vm.stopPrank();
    }

    function test_create_revertsOnInvalidDeadlines() public {
        uint64 nowTs = uint64(block.timestamp);
        vm.startPrank(resolver);
        // fund deadline in the past / now
        vm.expectRevert(ChallengeEscrow.InvalidDeadlines.selector);
        escrow.create(ID, alice, bob, STAKE, nowTs - 1, resolveDeadline);
        vm.expectRevert(ChallengeEscrow.InvalidDeadlines.selector);
        escrow.create(ID, alice, bob, STAKE, nowTs, resolveDeadline);
        // out of order / equal
        vm.expectRevert(ChallengeEscrow.InvalidDeadlines.selector);
        escrow.create(ID, alice, bob, STAKE, fundDeadline, fundDeadline);
        vm.expectRevert(ChallengeEscrow.InvalidDeadlines.selector);
        escrow.create(ID, alice, bob, STAKE, resolveDeadline, fundDeadline);
        vm.stopPrank();
    }

    // ---------------------------------------------------------------- deposit

    function test_deposit_firstPartyLocksStake() public {
        _create();
        vm.expectEmit(address(escrow));
        emit Deposited(ID, alice, STAKE);
        _deposit(alice);

        ChallengeEscrow.Challenge memory c = escrow.getChallenge(ID);
        assertTrue(c.depositedA);
        assertFalse(c.depositedB);
        assertEq(uint8(c.status), uint8(ChallengeEscrow.Status.Created));
        assertEq(usdc.balanceOf(address(escrow)), STAKE);
        assertEq(escrow.totalHeld(), STAKE);
        assertEq(usdc.balanceOf(alice), START_BALANCE - STAKE);
    }

    function test_deposit_secondPartyActivates() public {
        _create();
        _deposit(bob);
        vm.expectEmit(address(escrow));
        emit Deposited(ID, alice, STAKE);
        vm.expectEmit(address(escrow));
        emit Activated(ID, alice, bob, STAKE * 2);
        _deposit(alice);

        assertEq(uint8(_status(ID)), uint8(ChallengeEscrow.Status.Active));
        assertEq(usdc.balanceOf(address(escrow)), STAKE * 2);
        assertEq(escrow.totalHeld(), STAKE * 2);
    }

    function test_deposit_revertsForNonParty() public {
        _create();
        vm.prank(carol);
        vm.expectRevert(ChallengeEscrow.NotParty.selector);
        escrow.deposit(ID);
        // resolver is not a party either
        vm.prank(resolver);
        vm.expectRevert(ChallengeEscrow.NotParty.selector);
        escrow.deposit(ID);
    }

    function test_deposit_revertsOnDoubleDeposit() public {
        _create();
        _deposit(alice);
        vm.prank(alice);
        vm.expectRevert(ChallengeEscrow.AlreadyDeposited.selector);
        escrow.deposit(ID);
        assertEq(usdc.balanceOf(address(escrow)), STAKE);
    }

    function test_deposit_revertsOnceActive() public {
        _activate();
        vm.prank(alice);
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.Active));
        escrow.deposit(ID);
    }

    function test_deposit_revertsAtAndAfterFundDeadline() public {
        _create();
        vm.warp(fundDeadline);
        vm.prank(alice);
        vm.expectRevert(ChallengeEscrow.FundingClosed.selector);
        escrow.deposit(ID);
        vm.warp(fundDeadline + 1);
        vm.prank(alice);
        vm.expectRevert(ChallengeEscrow.FundingClosed.selector);
        escrow.deposit(ID);
    }

    function test_deposit_allowedOneSecondBeforeDeadline() public {
        _create();
        vm.warp(fundDeadline - 1);
        _deposit(alice);
        _deposit(bob);
        assertEq(uint8(_status(ID)), uint8(ChallengeEscrow.Status.Active));
    }

    function test_deposit_revertsOnUnknownId() public {
        vm.prank(alice);
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.None));
        escrow.deposit(ID);
    }

    function test_deposit_revertsWithoutAllowance() public {
        _create();
        vm.prank(alice);
        usdc.approve(address(escrow), STAKE - 1);
        vm.prank(alice);
        vm.expectRevert(ChallengeEscrow.TransferFailed.selector);
        escrow.deposit(ID);
        assertFalse(escrow.getChallenge(ID).depositedA);
        assertEq(escrow.totalHeld(), 0);
    }

    function test_deposit_revertsWithoutBalance() public {
        address poor = makeAddr("poor");
        vm.prank(poor);
        usdc.approve(address(escrow), type(uint256).max);
        vm.prank(resolver);
        escrow.create(ID, poor, bob, STAKE, fundDeadline, resolveDeadline);
        vm.prank(poor);
        vm.expectRevert(ChallengeEscrow.TransferFailed.selector);
        escrow.deposit(ID);
    }

    // ---------------------------------------------------------------- resolve

    function test_resolve_happyPathPaysWinner() public {
        _activate();
        vm.expectEmit(address(escrow));
        emit Resolved(ID, alice, bob, STAKE * 2);
        vm.prank(resolver);
        escrow.resolve(ID, alice);

        ChallengeEscrow.Challenge memory c = escrow.getChallenge(ID);
        assertEq(uint8(c.status), uint8(ChallengeEscrow.Status.Resolved));
        assertEq(c.winner, alice);
        assertEq(usdc.balanceOf(alice), START_BALANCE + STAKE);
        assertEq(usdc.balanceOf(bob), START_BALANCE - STAKE);
        _assertSettledClean();
    }

    function test_resolve_partyBCanWin() public {
        _activate();
        vm.expectEmit(address(escrow));
        emit Resolved(ID, bob, alice, STAKE * 2);
        vm.prank(resolver);
        escrow.resolve(ID, bob);
        assertEq(usdc.balanceOf(bob), START_BALANCE + STAKE);
        assertEq(usdc.balanceOf(alice), START_BALANCE - STAKE);
        _assertSettledClean();
    }

    function test_resolve_revertsForNonResolver() public {
        _activate();
        vm.prank(alice);
        vm.expectRevert(ChallengeEscrow.NotResolver.selector);
        escrow.resolve(ID, alice);
        vm.prank(carol);
        vm.expectRevert(ChallengeEscrow.NotResolver.selector);
        escrow.resolve(ID, alice);
    }

    function test_resolve_revertsBeforeFunded() public {
        _create();
        vm.prank(resolver);
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.Created));
        escrow.resolve(ID, alice);

        _deposit(alice); // half funded
        vm.prank(resolver);
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.Created));
        escrow.resolve(ID, alice);
    }

    function test_resolve_revertsOnUnknownId() public {
        vm.prank(resolver);
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.None));
        escrow.resolve(ID, alice);
    }

    function test_resolve_revertsForNonPartyWinner() public {
        _activate();
        vm.startPrank(resolver);
        vm.expectRevert(ChallengeEscrow.NotParty.selector);
        escrow.resolve(ID, carol);
        vm.expectRevert(ChallengeEscrow.NotParty.selector);
        escrow.resolve(ID, resolver);
        vm.expectRevert(ChallengeEscrow.NotParty.selector);
        escrow.resolve(ID, address(0));
        vm.stopPrank();
    }

    function test_resolve_revertsOnDoubleResolve() public {
        _activate();
        vm.prank(resolver);
        escrow.resolve(ID, alice);
        vm.prank(resolver);
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.Resolved));
        escrow.resolve(ID, bob);
        assertEq(usdc.balanceOf(alice), START_BALANCE + STAKE);
    }

    function test_resolve_allowedBeforeResolveDeadline() public {
        _activate();
        assertLt(block.timestamp, resolveDeadline);
        vm.prank(resolver);
        escrow.resolve(ID, alice);
        assertEq(uint8(_status(ID)), uint8(ChallengeEscrow.Status.Resolved));
    }

    function test_resolve_allowedAfterGraceIfNotRefunded() public {
        _activate();
        vm.warp(uint256(resolveDeadline) + grace + 30 days);
        vm.prank(resolver);
        escrow.resolve(ID, bob);
        assertEq(usdc.balanceOf(bob), START_BALANCE + STAKE);
        _assertSettledClean();
    }

    function test_resolve_revertsAfterRefund() public {
        _activate();
        vm.warp(uint256(resolveDeadline) + grace);
        escrow.refund(ID);
        vm.prank(resolver);
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.Refunded));
        escrow.resolve(ID, alice);
    }

    function test_resolve_revertsAfterVoid() public {
        _activate();
        vm.prank(resolver);
        escrow.resolveVoid(ID);
        vm.prank(resolver);
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.Voided));
        escrow.resolve(ID, alice);
    }

    // ------------------------------------------------------------ resolveVoid

    function test_resolveVoid_activeReturnsBothStakes() public {
        _activate();
        vm.expectEmit(address(escrow));
        emit Voided(ID, alice, bob, STAKE, STAKE);
        vm.prank(resolver);
        escrow.resolveVoid(ID);

        assertEq(uint8(_status(ID)), uint8(ChallengeEscrow.Status.Voided));
        assertEq(usdc.balanceOf(alice), START_BALANCE);
        assertEq(usdc.balanceOf(bob), START_BALANCE);
        _assertSettledClean();
    }

    function test_resolveVoid_createdReturnsOnlyWhatWasDeposited() public {
        _create();
        _deposit(bob);
        vm.expectEmit(address(escrow));
        emit Voided(ID, alice, bob, 0, STAKE);
        vm.prank(resolver);
        escrow.resolveVoid(ID);

        assertEq(usdc.balanceOf(alice), START_BALANCE);
        assertEq(usdc.balanceOf(bob), START_BALANCE);
        _assertSettledClean();
    }

    function test_resolveVoid_createdWithNoDeposits() public {
        _create();
        vm.prank(resolver);
        escrow.resolveVoid(ID);
        assertEq(uint8(_status(ID)), uint8(ChallengeEscrow.Status.Voided));
        // funding is closed for good
        vm.prank(alice);
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.Voided));
        escrow.deposit(ID);
    }

    function test_resolveVoid_revertsForNonResolver() public {
        _activate();
        vm.prank(alice);
        vm.expectRevert(ChallengeEscrow.NotResolver.selector);
        escrow.resolveVoid(ID);
    }

    function test_resolveVoid_revertsWhenTerminalOrUnknown() public {
        vm.prank(resolver);
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.None));
        escrow.resolveVoid(ID);

        _activate();
        vm.prank(resolver);
        escrow.resolve(ID, alice);
        vm.prank(resolver);
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.Resolved));
        escrow.resolveVoid(ID);
    }

    function test_resolveVoid_revertsOnDoubleVoid() public {
        _activate();
        vm.prank(resolver);
        escrow.resolveVoid(ID);
        vm.prank(resolver);
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.Voided));
        escrow.resolveVoid(ID);
        assertEq(usdc.balanceOf(alice), START_BALANCE);
    }

    // ----------------------------------------------------------------- refund

    function test_refund_revertsWhileFundingOpen() public {
        _create();
        _deposit(alice);
        vm.expectRevert(ChallengeEscrow.RefundNotAvailable.selector);
        escrow.refund(ID);
        vm.warp(fundDeadline - 1);
        vm.expectRevert(ChallengeEscrow.RefundNotAvailable.selector);
        escrow.refund(ID);
    }

    function test_refund_partiallyFundedAfterFundDeadline() public {
        _create();
        _deposit(alice);
        vm.warp(fundDeadline);
        vm.expectEmit(address(escrow));
        emit Refunded(ID, alice, bob, STAKE, 0);
        vm.prank(carol); // anyone
        escrow.refund(ID);

        assertEq(uint8(_status(ID)), uint8(ChallengeEscrow.Status.Refunded));
        assertEq(usdc.balanceOf(alice), START_BALANCE);
        assertEq(usdc.balanceOf(bob), START_BALANCE);
        assertEq(usdc.balanceOf(carol), START_BALANCE);
        _assertSettledClean();
    }

    function test_refund_unfundedAfterFundDeadline() public {
        _create();
        vm.warp(fundDeadline + 1);
        vm.expectEmit(address(escrow));
        emit Refunded(ID, alice, bob, 0, 0);
        escrow.refund(ID);
        assertEq(uint8(_status(ID)), uint8(ChallengeEscrow.Status.Refunded));
    }

    function test_refund_activeRevertsBeforeGraceEnds() public {
        _activate();
        // after the fund deadline, an Active challenge is not refundable
        vm.warp(fundDeadline + 1);
        vm.expectRevert(ChallengeEscrow.RefundNotAvailable.selector);
        escrow.refund(ID);
        vm.warp(resolveDeadline);
        vm.expectRevert(ChallengeEscrow.RefundNotAvailable.selector);
        escrow.refund(ID);
        vm.warp(uint256(resolveDeadline) + grace - 1);
        vm.expectRevert(ChallengeEscrow.RefundNotAvailable.selector);
        escrow.refund(ID);
    }

    function test_refund_activeAfterGraceReturnsBothStakes() public {
        _activate();
        vm.warp(uint256(resolveDeadline) + grace);
        vm.expectEmit(address(escrow));
        emit Refunded(ID, alice, bob, STAKE, STAKE);
        vm.prank(carol);
        escrow.refund(ID);

        assertEq(usdc.balanceOf(alice), START_BALANCE);
        assertEq(usdc.balanceOf(bob), START_BALANCE);
        assertEq(usdc.balanceOf(carol), START_BALANCE);
        _assertSettledClean();
    }

    function test_refund_revertsWhenTerminalOrUnknown() public {
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.None));
        escrow.refund(ID);

        _activate();
        vm.prank(resolver);
        escrow.resolve(ID, alice);
        vm.warp(uint256(resolveDeadline) + grace);
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.Resolved));
        escrow.refund(ID);
    }

    function test_refund_revertsOnDoubleRefund() public {
        _activate();
        vm.warp(uint256(resolveDeadline) + grace);
        escrow.refund(ID);
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.Refunded));
        escrow.refund(ID);
        assertEq(usdc.balanceOf(alice), START_BALANCE);
    }

    function test_refund_revertsAfterVoid() public {
        _activate();
        vm.prank(resolver);
        escrow.resolveVoid(ID);
        vm.warp(uint256(resolveDeadline) + grace);
        vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.Voided));
        escrow.refund(ID);
    }

    // ------------------------------------------- blocklisted recipient / claim

    function test_resolve_blockedWinnerIsDeferredThenClaimable() public {
        _activate();
        usdc.setBlocked(alice, true);

        vm.expectEmit(address(escrow));
        emit Resolved(ID, alice, bob, STAKE * 2);
        vm.expectEmit(address(escrow));
        emit PayoutDeferred(ID, alice, STAKE * 2);
        vm.prank(resolver);
        escrow.resolve(ID, alice);

        assertEq(uint8(_status(ID)), uint8(ChallengeEscrow.Status.Resolved));
        assertEq(escrow.owed(ID, alice), STAKE * 2);
        assertEq(escrow.totalHeld(), STAKE * 2);
        assertEq(usdc.balanceOf(address(escrow)), STAKE * 2);

        // still blocked: claim reverts and keeps the debt
        vm.expectRevert(ChallengeEscrow.TransferFailed.selector);
        escrow.claim(ID, alice);
        assertEq(escrow.owed(ID, alice), STAKE * 2);

        usdc.setBlocked(alice, false);
        vm.expectEmit(address(escrow));
        emit Claimed(ID, alice, STAKE * 2);
        vm.prank(carol); // anyone can trigger; funds go to alice
        escrow.claim(ID, alice);

        assertEq(usdc.balanceOf(alice), START_BALANCE + STAKE);
        assertEq(usdc.balanceOf(carol), START_BALANCE);
        assertEq(escrow.owed(ID, alice), 0);
        _assertSettledClean();
    }

    function test_refund_blockedPartyDoesNotTrapTheOther() public {
        _activate();
        usdc.setBlocked(alice, true);
        vm.warp(uint256(resolveDeadline) + grace);
        escrow.refund(ID);

        assertEq(usdc.balanceOf(bob), START_BALANCE, "bob refunded despite alice being blocked");
        assertEq(escrow.owed(ID, alice), STAKE);
        assertEq(escrow.owed(ID, bob), 0);
        assertEq(escrow.totalHeld(), STAKE);
        assertEq(usdc.balanceOf(address(escrow)), STAKE);
    }

    function test_resolveVoid_blockedPartyDoesNotTrapTheOther() public {
        _activate();
        usdc.setBlocked(bob, true);
        vm.prank(resolver);
        escrow.resolveVoid(ID);
        assertEq(usdc.balanceOf(alice), START_BALANCE);
        assertEq(escrow.owed(ID, bob), STAKE);
    }

    function test_claim_revertsWhenNothingOwed() public {
        _activate();
        vm.expectRevert(ChallengeEscrow.NothingOwed.selector);
        escrow.claim(ID, alice);
        vm.expectRevert(ChallengeEscrow.NothingOwed.selector);
        escrow.claim(ID, carol);
    }

    function test_claim_cannotBeRepeated() public {
        _activate();
        usdc.setBlocked(alice, true);
        vm.prank(resolver);
        escrow.resolve(ID, alice);
        usdc.setBlocked(alice, false);
        escrow.claim(ID, alice);
        vm.expectRevert(ChallengeEscrow.NothingOwed.selector);
        escrow.claim(ID, alice);
    }

    function test_deferredPayoutDoesNotLeakIntoOtherChallenges() public {
        // challenge 1: alice wins but is blocked, 2x stake parked
        _activate();
        usdc.setBlocked(alice, true);
        vm.prank(resolver);
        escrow.resolve(ID, alice);

        // challenge 2 between bob and carol settles normally and cannot touch alice's funds
        bytes32 id2 = keccak256("challenge-2");
        vm.prank(resolver);
        escrow.create(id2, bob, carol, STAKE, fundDeadline, resolveDeadline);
        vm.prank(bob);
        escrow.deposit(id2);
        vm.prank(carol);
        escrow.deposit(id2);
        vm.prank(resolver);
        escrow.resolve(id2, carol);

        assertEq(usdc.balanceOf(carol), START_BALANCE + STAKE);
        assertEq(usdc.balanceOf(address(escrow)), STAKE * 2);
        assertEq(escrow.totalHeld(), STAKE * 2);
        assertEq(escrow.owed(ID, alice), STAKE * 2);
    }

    // ------------------------------------------------------- isolation by id

    function test_challengesAreIndependent() public {
        bytes32 id2 = keccak256("challenge-2");
        _activate();
        vm.prank(resolver);
        escrow.create(id2, alice, carol, 10e6, fundDeadline, resolveDeadline);
        vm.prank(carol);
        escrow.deposit(id2);

        vm.prank(resolver);
        escrow.resolve(ID, bob);
        assertEq(usdc.balanceOf(address(escrow)), 10e6);
        assertEq(escrow.totalHeld(), 10e6);

        vm.warp(fundDeadline);
        escrow.refund(id2);
        assertEq(usdc.balanceOf(carol), START_BALANCE);
        _assertSettledClean();
    }

    // ------------------------------------------------- non-standard tokens

    function test_feeOnTransferToken_depositReverts() public {
        FeeOnTransferToken fot = new FeeOnTransferToken();
        ChallengeEscrow e = new ChallengeEscrow(address(fot), resolver);
        _fund(fot, e, alice);
        vm.prank(resolver);
        e.create(ID, alice, bob, STAKE, fundDeadline, resolveDeadline);

        vm.prank(alice);
        vm.expectRevert(
            abi.encodeWithSelector(ChallengeEscrow.UnexpectedTransferAmount.selector, STAKE, STAKE - STAKE / 100)
        );
        e.deposit(ID);

        assertFalse(e.getChallenge(ID).depositedA);
        assertEq(e.totalHeld(), 0);
        assertEq(fot.balanceOf(address(e)), 0);
        assertEq(fot.balanceOf(alice), START_BALANCE);
    }

    function test_falseReturningToken_depositReverts() public {
        FalseReturningToken frt = new FalseReturningToken();
        ChallengeEscrow e = new ChallengeEscrow(address(frt), resolver);
        _fund(frt, e, alice);
        vm.prank(resolver);
        e.create(ID, alice, bob, STAKE, fundDeadline, resolveDeadline);

        frt.setFailing(true);
        vm.prank(alice);
        vm.expectRevert(ChallengeEscrow.TransferFailed.selector);
        e.deposit(ID);
        assertFalse(e.getChallenge(ID).depositedA);
        assertEq(e.totalHeld(), 0);
    }

    function test_falseReturningToken_payoutIsDeferredNotLost() public {
        FalseReturningToken frt = new FalseReturningToken();
        ChallengeEscrow e = new ChallengeEscrow(address(frt), resolver);
        _fund(frt, e, alice);
        _fund(frt, e, bob);
        vm.prank(resolver);
        e.create(ID, alice, bob, STAKE, fundDeadline, resolveDeadline);
        vm.prank(alice);
        e.deposit(ID);
        vm.prank(bob);
        e.deposit(ID);

        frt.setFailing(true);
        vm.prank(resolver);
        e.resolve(ID, bob);
        assertEq(e.owed(ID, bob), STAKE * 2);
        assertEq(e.totalHeld(), STAKE * 2);
        assertEq(frt.balanceOf(address(e)), STAKE * 2);

        vm.expectRevert(ChallengeEscrow.TransferFailed.selector);
        e.claim(ID, bob);

        frt.setFailing(false);
        e.claim(ID, bob);
        assertEq(frt.balanceOf(bob), START_BALANCE + STAKE);
        assertEq(e.totalHeld(), 0);
        assertEq(frt.balanceOf(address(e)), 0);
    }

    function test_lyingToken_depositReverts() public {
        LyingToken lt = new LyingToken();
        ChallengeEscrow e = new ChallengeEscrow(address(lt), resolver);
        _fund(lt, e, alice);
        vm.prank(resolver);
        e.create(ID, alice, bob, STAKE, fundDeadline, resolveDeadline);
        vm.prank(alice);
        vm.expectRevert(abi.encodeWithSelector(ChallengeEscrow.UnexpectedTransferAmount.selector, STAKE, 0));
        e.deposit(ID);
    }

    function test_tokenWithoutCode_depositReverts() public {
        ChallengeEscrow e = new ChallengeEscrow(makeAddr("not-a-token"), resolver);
        vm.prank(resolver);
        e.create(ID, alice, bob, STAKE, fundDeadline, resolveDeadline);
        vm.prank(alice);
        vm.expectRevert(ChallengeEscrow.TransferFailed.selector);
        e.deposit(ID);
    }

    // ------------------------------------------------------------- reentrancy

    function test_reentrancy_refundDuringPayoutIsBlocked() public {
        ReentrantToken rt = new ReentrantToken();
        ChallengeEscrow e = new ChallengeEscrow(address(rt), resolver);
        _fund(rt, e, alice);
        _fund(rt, e, bob);
        vm.prank(resolver);
        e.create(ID, alice, bob, STAKE, fundDeadline, resolveDeadline);
        vm.prank(alice);
        e.deposit(ID);
        vm.prank(bob);
        e.deposit(ID);

        vm.warp(uint256(resolveDeadline) + grace);
        rt.arm(address(e), abi.encodeCall(ChallengeEscrow.refund, (ID)));
        e.refund(ID);

        assertFalse(rt.lastCallOk(), "reentrant call must fail");
        assertEq(bytes4(rt.lastCallReturn()), ChallengeEscrow.Reentrancy.selector);
        assertEq(rt.balanceOf(alice), START_BALANCE);
        assertEq(rt.balanceOf(bob), START_BALANCE);
        assertEq(rt.balanceOf(address(e)), 0);
        assertEq(e.totalHeld(), 0);
    }

    function test_reentrancy_depositDuringDepositIsBlocked() public {
        ReentrantToken rt = new ReentrantToken();
        ChallengeEscrow e = new ChallengeEscrow(address(rt), resolver);
        _fund(rt, e, alice);
        _fund(rt, e, bob);
        vm.prank(resolver);
        e.create(ID, alice, bob, STAKE, fundDeadline, resolveDeadline);

        rt.arm(address(e), abi.encodeCall(ChallengeEscrow.claim, (ID, alice)));
        vm.prank(alice);
        e.deposit(ID);

        assertFalse(rt.lastCallOk());
        assertEq(bytes4(rt.lastCallReturn()), ChallengeEscrow.Reentrancy.selector);
        assertEq(rt.balanceOf(address(e)), STAKE);
        assertEq(e.totalHeld(), STAKE);
    }

    // ------------------------------------------------------------------- fuzz

    function testFuzz_happyPath_conservesFunds(uint256 stake, bool aWins) public {
        stake = bound(stake, 1, type(uint128).max);
        usdc.mint(alice, stake);
        usdc.mint(bob, stake);
        uint256 aBefore = usdc.balanceOf(alice);
        uint256 bBefore = usdc.balanceOf(bob);

        vm.prank(resolver);
        escrow.create(ID, alice, bob, stake, fundDeadline, resolveDeadline);
        _deposit(alice);
        _deposit(bob);
        assertEq(usdc.balanceOf(address(escrow)), stake * 2);

        vm.prank(resolver);
        escrow.resolve(ID, aWins ? alice : bob);

        assertEq(usdc.balanceOf(alice), aWins ? aBefore + stake : aBefore - stake);
        assertEq(usdc.balanceOf(bob), aWins ? bBefore - stake : bBefore + stake);
        _assertSettledClean();
    }

    function testFuzz_voidAndRefund_returnExactStakes(uint256 stake, bool depA, bool depB, bool useVoid) public {
        stake = bound(stake, 1, type(uint128).max);
        usdc.mint(alice, stake);
        usdc.mint(bob, stake);
        uint256 aBefore = usdc.balanceOf(alice);
        uint256 bBefore = usdc.balanceOf(bob);

        vm.prank(resolver);
        escrow.create(ID, alice, bob, stake, fundDeadline, resolveDeadline);
        if (depA) _deposit(alice);
        if (depB) _deposit(bob);

        if (useVoid) {
            vm.prank(resolver);
            escrow.resolveVoid(ID);
        } else {
            vm.warp(uint256(resolveDeadline) + grace);
            escrow.refund(ID);
        }

        assertEq(usdc.balanceOf(alice), aBefore);
        assertEq(usdc.balanceOf(bob), bBefore);
        _assertSettledClean();
    }

    function testFuzz_create_deadlineValidation(uint64 nowTs, uint64 fd, uint64 rd) public {
        nowTs = uint64(bound(nowTs, 1, type(uint64).max));
        vm.warp(nowTs);
        bool valid = fd > nowTs && rd > fd && uint256(fd) - nowTs <= escrow.MAX_FUND_WINDOW()
            && uint256(rd) - fd <= escrow.MAX_RESOLVE_WINDOW();

        vm.prank(resolver);
        if (!valid) vm.expectRevert(ChallengeEscrow.InvalidDeadlines.selector);
        escrow.create(ID, alice, bob, STAKE, fd, rd);

        assertEq(uint8(_status(ID)), valid ? uint8(ChallengeEscrow.Status.Created) : uint8(ChallengeEscrow.Status.None));
    }

    function testFuzz_deposit_onlyBeforeFundDeadline(uint64 at) public {
        _create();
        at = uint64(bound(at, block.timestamp, uint256(resolveDeadline) + 365 days));
        vm.warp(at);

        vm.prank(alice);
        if (at >= fundDeadline) vm.expectRevert(ChallengeEscrow.FundingClosed.selector);
        escrow.deposit(ID);

        assertEq(escrow.getChallenge(ID).depositedA, at < fundDeadline);
    }

    function testFuzz_refund_partialFundingWindow(uint64 at) public {
        _create();
        _deposit(alice);
        at = uint64(bound(at, block.timestamp, uint256(resolveDeadline) + 365 days));
        vm.warp(at);

        bool allowed = at >= fundDeadline;
        if (!allowed) vm.expectRevert(ChallengeEscrow.RefundNotAvailable.selector);
        escrow.refund(ID);

        assertEq(usdc.balanceOf(alice), allowed ? START_BALANCE : START_BALANCE - STAKE);
    }

    function testFuzz_refund_activeWindow(uint64 at) public {
        _activate();
        at = uint64(bound(at, block.timestamp, uint256(resolveDeadline) + 365 days));
        vm.warp(at);

        bool allowed = at >= uint256(resolveDeadline) + grace;
        if (!allowed) vm.expectRevert(ChallengeEscrow.RefundNotAvailable.selector);
        escrow.refund(ID);

        assertEq(usdc.balanceOf(address(escrow)), allowed ? 0 : STAKE * 2);
        // the resolver can still settle if and only if the refund has not been used
        vm.prank(resolver);
        if (allowed) vm.expectRevert(_wrongStatus(ChallengeEscrow.Status.Refunded));
        escrow.resolve(ID, alice);
        _assertSettledClean();
    }

    function testFuzz_resolve_onlyResolver(address caller) public {
        vm.assume(caller != resolver);
        _activate();
        vm.prank(caller);
        vm.expectRevert(ChallengeEscrow.NotResolver.selector);
        escrow.resolve(ID, alice);
        vm.prank(caller);
        vm.expectRevert(ChallengeEscrow.NotResolver.selector);
        escrow.resolveVoid(ID);
        vm.prank(caller);
        vm.expectRevert(ChallengeEscrow.NotResolver.selector);
        escrow.create(keccak256("x"), alice, bob, STAKE, fundDeadline, resolveDeadline);
    }

    function testFuzz_resolve_winnerMustBeParty(address winner) public {
        vm.assume(winner != alice && winner != bob);
        _activate();
        vm.prank(resolver);
        vm.expectRevert(ChallengeEscrow.NotParty.selector);
        escrow.resolve(ID, winner);
    }

    function testFuzz_deposit_onlyParties(address caller) public {
        vm.assume(caller != alice && caller != bob);
        _create();
        vm.prank(caller);
        vm.expectRevert(ChallengeEscrow.NotParty.selector);
        escrow.deposit(ID);
    }
}
