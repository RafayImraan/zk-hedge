// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {ShieldedSolverEscrow} from "../src/ShieldedSolverEscrow.sol";

interface VmDeploy {
    function envUint(string calldata name) external returns (uint256);
    function startBroadcast(uint256 privateKey) external;
    function stopBroadcast() external;
}

/// @notice Deploys the stateless escrow implementation to Arbitrum Sepolia.
/// @dev Zcash addresses and viewing keys are intentionally not deployed or
///      stored here. They are private, off-chain relayer inputs.
contract DeployShieldedEscrow {
    VmDeploy private constant vm = VmDeploy(address(uint160(uint256(keccak256("hevm cheat code")))));
    uint256 private constant ARBITRUM_SEPOLIA_CHAIN_ID = 421_614;
 
    error WrongChain(uint256 actualChainId);

    function run() external returns (ShieldedSolverEscrow escrow) {
        if (block.chainid != ARBITRUM_SEPOLIA_CHAIN_ID) revert WrongChain(block.chainid);

        uint256 deployerPrivateKey = vm.envUint("DEPLOYER_PRIVATE_KEY");
        vm.startBroadcast(deployerPrivateKey);
        escrow = new ShieldedSolverEscrow();
        vm.stopBroadcast();
    }
}
