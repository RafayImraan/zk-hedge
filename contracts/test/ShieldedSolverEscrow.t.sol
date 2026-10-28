// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {ShieldedSolverEscrow} from "../src/ShieldedSolverEscrow.sol";

interface Vm {
    struct Log {
        bytes32[] topics;
        bytes data;
        address emitter;
    }

    function recordLogs() external;
    function getRecordedLogs() external returns (Log[] memory);
}

contract MockUSDC is ERC20 {
    constructor() ERC20("Mock USD Coin", "mUSDC") {}

    function decimals() public pure override returns (uint8) {
        return 6;
    }

    function mint(address account, uint256 amount) external {
        _mint(account, amount);
    }
}

contract EscrowCaller {
    function fulfill(ShieldedSolverEscrow escrow, bytes32 intentHash, address recipient, uint256 amount) external {
        escrow.fulfillSwap(intentHash, recipient, amount);
    }
}

/// @dev Uses Solidity require assertions so the suite does not need forge-std.
contract ShieldedSolverEscrowTest {
    Vm private constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));
    uint256 private constant DEPOSIT = 25_000_000; // 25 mock USDC (6 decimals)
    address private constant USER = address(0xBEEF);
    bytes32 private constant INTENT_HASH = keccak256("zk-hedge-intent-1");

    function testSolverDepositsAndFulfillsIntentOnce() external {
        (ShieldedSolverEscrow escrow, MockUSDC token) = _fundIntent();

        vm.recordLogs();
        escrow.fulfillSwap(INTENT_HASH, USER, DEPOSIT);

        require(token.balanceOf(USER) == DEPOSIT, "recipient did not receive escrow");
        require(token.balanceOf(address(escrow)) == 0, "escrow balance should be zero");
        require(escrow.executed(INTENT_HASH), "execution state was not recorded");
        _assertSwapExecutedLog(address(escrow));

        (bool secondCallSucceeded,) =
            address(escrow).call(abi.encodeCall(escrow.fulfillSwap, (INTENT_HASH, USER, DEPOSIT)));
        require(!secondCallSucceeded, "intent must not be fulfilled twice");
    }

    function testCannotRedirectRecipientOrAmount() external {
        (ShieldedSolverEscrow escrow, MockUSDC token) = _fundIntent();

        (bool wrongRecipientSucceeded,) =
            address(escrow).call(abi.encodeCall(escrow.fulfillSwap, (INTENT_HASH, address(0xCAFE), DEPOSIT)));
        require(!wrongRecipientSucceeded, "recipient must match committed recipient");

        (bool wrongAmountSucceeded,) =
            address(escrow).call(abi.encodeCall(escrow.fulfillSwap, (INTENT_HASH, USER, DEPOSIT - 1)));
        require(!wrongAmountSucceeded, "amount must match escrowed amount");
        require(token.balanceOf(address(escrow)) == DEPOSIT, "failed calls must preserve escrow");
    }

    function testOnlyDepositingSolverCanFulfill() external {
        (ShieldedSolverEscrow escrow,) = _fundIntent();
        EscrowCaller attacker = new EscrowCaller();

        (bool succeeded,) =
            address(attacker).call(abi.encodeCall(attacker.fulfill, (escrow, INTENT_HASH, USER, DEPOSIT)));
        require(!succeeded, "non-solver must not fulfill intent");
    }

    function testSolverCanCancelAndReclaimUnfulfilledIntent() external {
        (ShieldedSolverEscrow escrow, MockUSDC token) = _fundIntent();

        escrow.cancelIntent(INTENT_HASH);

        require(token.balanceOf(address(this)) == DEPOSIT, "solver did not receive refund");
        require(token.balanceOf(address(escrow)) == 0, "escrow balance should be zero");

        (bool fulfillSucceeded,) =
            address(escrow).call(abi.encodeCall(escrow.fulfillSwap, (INTENT_HASH, USER, DEPOSIT)));
        require(!fulfillSucceeded, "cancelled intent must not be fulfilled");
    }

    function testCannotReuseIntentHash() external {
        (ShieldedSolverEscrow escrow, MockUSDC token) = _fundIntent();
        token.approve(address(escrow), DEPOSIT);

        (bool succeeded,) =
            address(escrow).call(abi.encodeCall(escrow.depositIntent, (token, INTENT_HASH, USER, DEPOSIT)));
        require(!succeeded, "intent hash must not be reusable");
    }

    function _fundIntent() private returns (ShieldedSolverEscrow escrow, MockUSDC token) {
        escrow = new ShieldedSolverEscrow();
        token = new MockUSDC();
        token.mint(address(this), DEPOSIT);
        token.approve(address(escrow), DEPOSIT);
        escrow.depositIntent(token, INTENT_HASH, USER, DEPOSIT);
        require(token.balanceOf(address(escrow)) == DEPOSIT, "deposit missing from escrow");
    }

    function _assertSwapExecutedLog(address expectedEmitter) private {
        Vm.Log[] memory logs = vm.getRecordedLogs();
        bytes32 eventSignature = keccak256("SwapExecuted(bytes32,address,uint256)");
        bool found;

        for (uint256 i; i < logs.length; i++) {
            if (logs[i].emitter != expectedEmitter || logs[i].topics.length != 3 || logs[i].topics[0] != eventSignature)
            {
                continue;
            }

            require(logs[i].topics[1] == INTENT_HASH, "wrong event intent hash");
            address recipient = address(uint160(uint256(logs[i].topics[2])));
            require(logs[i].topics[2] == bytes32(uint256(uint160(USER))), "wrong indexed recipient");
            uint256 amount = abi.decode(logs[i].data, (uint256));
            require(recipient == USER, "wrong event recipient");
            require(amount == DEPOSIT, "wrong event amount");
            found = true;
        }

        require(found, "SwapExecuted event missing");
    }
}
