// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {ShieldedSolverEscrow} from "../src/ShieldedSolverEscrow.sol";
import {DemoUSDC} from "../src/DemoUSDC.sol";

interface VmDemo {
    function envUint(string calldata name) external returns (uint256);
    function envBytes32(string calldata name) external returns (bytes32);
    function envAddress(string calldata name) external returns (address);
    function addr(uint256 privateKey) external returns (address);
    function startBroadcast(uint256 privateKey) external;
    function stopBroadcast() external;
}

/// @notice Creates demo liquidity, escrows it, and fulfills one signed intent.
/// @dev Uses a newly deployed, unrestricted-faucet test token. Never use as USDC.
contract RunDemoSwap {
    VmDemo private constant vm = VmDemo(address(uint160(uint256(keccak256("hevm cheat code")))));
    uint256 private constant ARBITRUM_SEPOLIA_CHAIN_ID = 421_614;
    uint256 private constant DEMO_AMOUNT = 25_000_000; // 25.00 tokens at 6 decimals

    error WrongChain(uint256 actualChainId);

    function run() external returns (DemoUSDC token, bytes32 intentHash) {
        if (block.chainid != ARBITRUM_SEPOLIA_CHAIN_ID) revert WrongChain(block.chainid);

        uint256 solverKey = vm.envUint("SOLVER_PRIVATE_KEY");
        address solver = vm.addr(solverKey);
        address escrowAddress = vm.envAddress("SHIELDED_SOLVER_ESCROW_ADDRESS");
        address recipient = vm.envAddress("DEMO_EVM_RECIPIENT");
        intentHash = vm.envBytes32("INTENT_HASH");
        require(escrowAddress.code.length > 0, "escrow address has no code");
        require(recipient != address(0), "recipient is zero");
        require(intentHash != bytes32(0), "intent hash is zero");

        vm.startBroadcast(solverKey);
        token = new DemoUSDC();
        token.faucet(solver, DEMO_AMOUNT);
        token.approve(escrowAddress, DEMO_AMOUNT);
        ShieldedSolverEscrow escrow = ShieldedSolverEscrow(escrowAddress);
        escrow.depositIntent(token, intentHash, recipient, DEMO_AMOUNT);
        escrow.fulfillSwap(intentHash, recipient, DEMO_AMOUNT);
        vm.stopBroadcast();
    }
}
