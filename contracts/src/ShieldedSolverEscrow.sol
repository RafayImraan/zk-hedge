// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {IERC20} from "@openzeppelin/contracts/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin/contracts/token/ERC20/utils/SafeERC20.sol";
import {ReentrancyGuard} from "@openzeppelin/contracts/utils/ReentrancyGuard.sol";

/// @title ShieldedSolverEscrow
/// @notice Holds solver-provided ERC-20 liquidity for one intent and releases it
///         to the recipient committed when the solver deposits.
contract ShieldedSolverEscrow is ReentrancyGuard {
    using SafeERC20 for IERC20;

    struct Intent {
        address solver;
        IERC20 token;
        address recipient;
        uint256 amount;
        bool fulfilled;
        bool cancelled;
    }

    mapping(bytes32 intentHash => Intent intent) public intents;
    mapping(bytes32 intentHash => bool) public executed;

    error ZeroAddress();
    error ZeroAmount();
    error EmptyIntentHash();
    error IntentAlreadyUsed(bytes32 intentHash);
    error IntentNotFound(bytes32 intentHash);
    error NotSolver(address caller, address solver);
    error WrongRecipient(address expected, address provided);
    error WrongAmount(uint256 expected, uint256 provided);
    error IntentAlreadyClosed(bytes32 intentHash);
    error UnsupportedTransferFee(uint256 expected, uint256 received);

    event IntentDeposited(
        bytes32 indexed intentHash, address indexed solver, address indexed token, address recipient, uint256 amount
    );
    event SwapExecuted(bytes32 indexed intentHash, address indexed recipient, uint256 amount);
    event IntentCancelled(bytes32 indexed intentHash, address indexed solver, uint256 amount);

    /// @notice Escrows an exact ERC-20 amount and commits its eventual recipient.
    /// @dev A hash can only be used once, including after cancellation or fulfillment.
    function depositIntent(IERC20 token, bytes32 intentHash, address recipient, uint256 amount) external nonReentrant {
        if (address(token) == address(0) || recipient == address(0)) revert ZeroAddress();
        if (intentHash == bytes32(0)) revert EmptyIntentHash();
        if (amount == 0) revert ZeroAmount();
        if (intents[intentHash].solver != address(0)) revert IntentAlreadyUsed(intentHash);

        uint256 balanceBefore = token.balanceOf(address(this));
        token.safeTransferFrom(msg.sender, address(this), amount);
        uint256 received = token.balanceOf(address(this)) - balanceBefore;
        if (received != amount) revert UnsupportedTransferFee(amount, received);

        intents[intentHash] = Intent({
            solver: msg.sender, token: token, recipient: recipient, amount: amount, fulfilled: false, cancelled: false
        });

        emit IntentDeposited(intentHash, msg.sender, address(token), recipient, amount);
    }

    /// @notice Pays the committed recipient the full escrowed amount.
    /// @dev Only the depositing solver can trigger settlement; the recipient and
    ///      amount must match the values committed at deposit time.
    function fulfillSwap(bytes32 intentHash, address recipient, uint256 amount) external nonReentrant {
        Intent storage intent = intents[intentHash];
        if (intent.solver == address(0)) revert IntentNotFound(intentHash);
        if (msg.sender != intent.solver) revert NotSolver(msg.sender, intent.solver);
        if (intent.fulfilled || intent.cancelled) revert IntentAlreadyClosed(intentHash);
        if (recipient != intent.recipient) revert WrongRecipient(intent.recipient, recipient);
        if (amount != intent.amount) revert WrongAmount(intent.amount, amount);

        intent.fulfilled = true;
        executed[intentHash] = true;
        intent.token.safeTransfer(intent.recipient, intent.amount);

        emit SwapExecuted(intentHash, intent.recipient, intent.amount);
    }

    /// @notice Lets the depositing solver reclaim an intent that has not settled.
    function cancelIntent(bytes32 intentHash) external nonReentrant {
        Intent storage intent = intents[intentHash];
        if (intent.solver == address(0)) revert IntentNotFound(intentHash);
        if (msg.sender != intent.solver) revert NotSolver(msg.sender, intent.solver);
        if (intent.fulfilled || intent.cancelled) revert IntentAlreadyClosed(intentHash);

        intent.cancelled = true;
        intent.token.safeTransfer(intent.solver, intent.amount);

        emit IntentCancelled(intentHash, intent.solver, intent.amount);
    }
}
