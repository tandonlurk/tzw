// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {ChallengeEscrow} from "../src/ChallengeEscrow.sol";
import {AgentVault, VaultFactory} from "../src/AgentVault.sol";
import {MockUSDC} from "./mocks/MockTokens.sol";

contract AgentVaultTest is Test {
    MockUSDC usdc;
    ChallengeEscrow escrow;
    VaultFactory factory;
    AgentVault va; // alice's agent
    AgentVault vb; // bob's agent

    address resolver = makeAddr("resolver");
    address operator = makeAddr("operator");
    address alice = makeAddr("alice");
    address bob = makeAddr("bob");
    address mallory = makeAddr("mallory");

    bytes32 constant ID = keccak256("challenge-1");
    uint256 constant STAKE = 5e6;
    uint256 constant MAX_PER = 10e6;
    uint256 constant MAX_EXP = 15e6;
    uint256 constant FUNDED = 100e6;

    function setUp() public {
        vm.warp(1_800_000_000);
        usdc = new MockUSDC();
        escrow = new ChallengeEscrow(address(usdc), resolver);
        factory = new VaultFactory(address(usdc), address(escrow));
        vm.prank(alice);
        va = factory.createVault(operator, MAX_PER, MAX_EXP, bytes32("agent-a"));
        vm.prank(bob);
        vb = factory.createVault(operator, MAX_PER, MAX_EXP, bytes32("agent-b"));
        usdc.mint(address(va), FUNDED);
        usdc.mint(address(vb), FUNDED);
    }

    function _create(bytes32 id, address a, address b, uint256 stake) internal {
        vm.prank(resolver);
        escrow.create(id, a, b, stake, uint64(block.timestamp + 10 minutes), uint64(block.timestamp + 1 hours));
    }

    function _stakeBoth(bytes32 id) internal {
        vm.startPrank(operator);
        va.stake(id);
        vb.stake(id);
        vm.stopPrank();
    }

    // ---------------------------------------------------------------- factory

    function test_factory_setsOwnerOperatorLimits() public view {
        assertEq(va.OWNER(), alice);
        assertEq(va.operator(), operator);
        assertEq(va.maxPerChallenge(), MAX_PER);
        assertEq(va.maxExposure(), MAX_EXP);
        assertEq(address(va.ESCROW()), address(escrow));
        assertTrue(factory.isVault(address(va)));
        assertFalse(factory.isVault(alice));
    }

    // ---------------------------------------------------------------- full lifecycle

    function test_lifecycle_winnerVaultGetsPot_thenRelease() public {
        _create(ID, address(va), address(vb), STAKE);
        _stakeBoth(ID);
        assertEq(uint8(escrow.getChallenge(ID).status), uint8(ChallengeEscrow.Status.Active));
        assertEq(va.exposure(), STAKE);
        assertEq(usdc.balanceOf(address(va)), FUNDED - STAKE);
        assertEq(usdc.allowance(address(va), address(escrow)), 0);

        vm.prank(resolver);
        escrow.resolve(ID, address(va));
        assertEq(usdc.balanceOf(address(va)), FUNDED + STAKE);
        assertEq(usdc.balanceOf(address(vb)), FUNDED - STAKE);

        vm.prank(mallory); // anyone can release; it moves no funds
        va.release(ID);
        vb.release(ID);
        assertEq(va.exposure(), 0);
        assertEq(vb.exposure(), 0);
        assertEq(va.staked(ID), 0);
    }

    function test_voidAndRefund_returnStakesToVaults() public {
        _create(ID, address(va), address(vb), STAKE);
        _stakeBoth(ID);
        vm.prank(resolver);
        escrow.resolveVoid(ID);
        assertEq(usdc.balanceOf(address(va)), FUNDED);
        va.release(ID);

        bytes32 id2 = keccak256("c2");
        _create(id2, address(va), address(vb), STAKE);
        vm.prank(operator);
        va.stake(id2); // only one side funds
        vm.warp(block.timestamp + 10 minutes);
        escrow.refund(id2);
        assertEq(usdc.balanceOf(address(va)), FUNDED);
        va.release(id2);
        assertEq(va.exposure(), 0);
    }

    // ---------------------------------------------------------------- operator limits

    function test_stake_onlyOperator() public {
        _create(ID, address(va), address(vb), STAKE);
        vm.expectRevert(AgentVault.NotOperator.selector);
        vm.prank(alice); // even the owner cannot stake: staking is the operator's role
        va.stake(ID);
        vm.expectRevert(AgentVault.NotOperator.selector);
        vm.prank(mallory);
        va.stake(ID);
    }

    function test_stake_rejectsChallengeWithoutThisVault() public {
        address other = makeAddr("other");
        _create(ID, other, address(vb), STAKE);
        vm.expectRevert(AgentVault.NotAParty.selector);
        vm.prank(operator);
        va.stake(ID);
    }

    function test_stake_rejectsUnknownOrActiveChallenge() public {
        vm.expectRevert(abi.encodeWithSelector(AgentVault.WrongStatus.selector, ChallengeEscrow.Status.None));
        vm.prank(operator);
        va.stake(ID);
    }

    function test_stake_twiceReverts() public {
        _create(ID, address(va), address(vb), STAKE);
        vm.startPrank(operator);
        va.stake(ID);
        vm.expectRevert(AgentVault.AlreadyStaked.selector);
        va.stake(ID);
        vm.stopPrank();
    }

    function test_stake_perChallengeCap() public {
        _create(ID, address(va), address(vb), MAX_PER + 1);
        vm.expectRevert(abi.encodeWithSelector(AgentVault.OverPerChallenge.selector, MAX_PER + 1, MAX_PER));
        vm.prank(operator);
        va.stake(ID);
    }

    function test_stake_exposureCap() public {
        _create(keccak256("1"), address(va), address(vb), MAX_PER);
        _create(keccak256("2"), address(va), address(vb), MAX_PER);
        vm.startPrank(operator);
        va.stake(keccak256("1"));
        vm.expectRevert(abi.encodeWithSelector(AgentVault.OverExposure.selector, 2 * MAX_PER, MAX_EXP));
        va.stake(keccak256("2"));
        vm.stopPrank();
    }

    function test_stake_pausedReverts_andUnpause() public {
        _create(ID, address(va), address(vb), STAKE);
        vm.prank(alice);
        va.setPaused(true);
        vm.expectRevert(AgentVault.IsPaused.selector);
        vm.prank(operator);
        va.stake(ID);
        vm.prank(alice);
        va.setPaused(false);
        vm.prank(operator);
        va.stake(ID);
    }

    function test_stake_insufficientBalanceReverts() public {
        vm.prank(alice);
        va.withdraw(alice, FUNDED);
        _create(ID, address(va), address(vb), STAKE);
        vm.expectRevert();
        vm.prank(operator);
        va.stake(ID);
        assertEq(va.exposure(), 0);
    }

    function test_revokedOperatorCannotStake() public {
        _create(ID, address(va), address(vb), STAKE);
        vm.prank(alice);
        va.setOperator(address(0));
        vm.expectRevert(AgentVault.NotOperator.selector);
        vm.prank(operator);
        va.stake(ID);
    }

    // ---------------------------------------------------------------- owner powers

    function test_onlyOwner_admin() public {
        vm.startPrank(operator);
        vm.expectRevert(AgentVault.NotOwner.selector);
        va.withdraw(operator, 1);
        vm.expectRevert(AgentVault.NotOwner.selector);
        va.setOperator(operator);
        vm.expectRevert(AgentVault.NotOwner.selector);
        va.setLimits(type(uint256).max, type(uint256).max);
        vm.expectRevert(AgentVault.NotOwner.selector);
        va.setPaused(false);
        vm.stopPrank();
    }

    function test_ownerWithdrawsAnytime() public {
        vm.prank(alice);
        va.withdraw(bob, 40e6);
        assertEq(usdc.balanceOf(bob), 40e6);
        assertEq(va.balance(), FUNDED - 40e6);
    }

    function test_release_notTerminalReverts() public {
        _create(ID, address(va), address(vb), STAKE);
        _stakeBoth(ID);
        vm.expectRevert(abi.encodeWithSelector(AgentVault.WrongStatus.selector, ChallengeEscrow.Status.Active));
        va.release(ID);
        vm.expectRevert(AgentVault.NotStaked.selector);
        va.release(keccak256("never"));
    }

    // ---------------------------------------------------------------- fuzz

    /// The operator alone can never move vault funds anywhere but into escrow for this vault's challenges;
    /// the vault balance plus its escrowed stakes stays constant until the resolver acts.
    function testFuzz_operatorCannotDrain(uint256 stake, uint8 n) public {
        stake = bound(stake, 1, MAX_PER);
        n = uint8(bound(n, 1, 5));
        uint256 committed;
        for (uint8 i = 0; i < n; i++) {
            bytes32 id = keccak256(abi.encode(i));
            _create(id, address(va), address(vb), stake);
            vm.prank(operator);
            try va.stake(id) {
                committed += stake;
            } catch {}
        }
        assertLe(committed, MAX_EXP);
        assertEq(va.exposure(), committed);
        assertEq(usdc.balanceOf(address(va)) + committed, FUNDED);
    }
}
