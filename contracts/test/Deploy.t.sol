// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Test} from "forge-std/Test.sol";
import {Deploy} from "../script/Deploy.s.sol";
import {ChallengeEscrow} from "../src/ChallengeEscrow.sol";
import {VaultFactory} from "../src/AgentVault.sol";
import {MockUSDC} from "./mocks/MockTokens.sol";

contract DeployTest is Test {
    address resolver = makeAddr("resolver");

    /// @dev One test on purpose: env vars are process-wide and forge runs tests in parallel.
    function test_deploy_readsEnvAndChecksToken() public {
        Deploy d = new Deploy();
        vm.setEnv("RESOLVER_ADDRESS", vm.toString(resolver));

        vm.setEnv("USDC_ADDRESS", vm.toString(makeAddr("not-a-token")));
        vm.expectRevert(Deploy.TokenIsNotSixDecimals.selector);
        d.run();

        MockUSDC usdc = new MockUSDC();
        vm.setEnv("USDC_ADDRESS", vm.toString(address(usdc)));
        (ChallengeEscrow escrow, VaultFactory factory) = d.run();
        assertEq(escrow.USDC(), address(usdc));
        assertEq(escrow.RESOLVER(), resolver);
        assertEq(factory.ESCROW(), address(escrow));
        assertEq(factory.USDC(), address(usdc));
    }

    function test_deploy_refusesOtherChains() public {
        Deploy d = new Deploy();
        vm.chainId(5042); // Arc mainnet
        vm.expectRevert(abi.encodeWithSelector(Deploy.UnsupportedChain.selector, 5042));
        d.run();
        vm.chainId(1);
        vm.expectRevert(abi.encodeWithSelector(Deploy.UnsupportedChain.selector, 1));
        d.run();
    }
}
