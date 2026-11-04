// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";

/// @notice Unrestricted faucet token for Arbitrum Sepolia demos only.
contract DemoUSDC is ERC20 {
    constructor() ERC20("ZK-Hedge Demo USDC", "dUSDC") {}

    function faucet(address recipient, uint256 amount) external {
        _mint(recipient, amount);
    }
}
