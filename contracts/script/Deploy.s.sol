// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {Script, console} from "forge-std/Script.sol";
import {ChallengeEscrow} from "../src/ChallengeEscrow.sol";

/// @notice Deploys ChallengeEscrow. Reads USDC_ADDRESS and RESOLVER_ADDRESS from the environment.
///         The deployer key is never read from a file or env var here: pass it on the command line
///         with a Foundry keystore (`--account <name>`).
/// @dev Refuses to run anywhere except Arc testnet and a local dev chain. Mainnet deployment needs
///      explicit owner approval (AGENTS.md 7.6) and a deliberate change to this script.
contract Deploy is Script {
    uint256 constant ARC_TESTNET = 5042002;
    uint256 constant LOCAL = 31337;

    error UnsupportedChain(uint256 chainId);
    error TokenIsNotSixDecimals();

    function run() external returns (ChallengeEscrow escrow) {
        if (block.chainid != ARC_TESTNET && block.chainid != LOCAL) revert UnsupportedChain(block.chainid);

        address usdc = vm.envAddress("USDC_ADDRESS");
        address resolver = vm.envAddress("RESOLVER_ADDRESS");

        (bool ok, bytes memory ret) = usdc.staticcall(abi.encodeWithSignature("decimals()"));
        if (!ok || ret.length < 32 || abi.decode(ret, (uint8)) != 6) revert TokenIsNotSixDecimals();

        vm.startBroadcast();
        escrow = new ChallengeEscrow(usdc, resolver);
        vm.stopBroadcast();

        console.log("ChallengeEscrow:", address(escrow));
        console.log("USDC:", usdc);
        console.log("Resolver:", resolver);
    }
}
