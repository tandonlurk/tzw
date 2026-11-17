// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @dev Minimal 6-decimal ERC-20 with a USDC-style blocklist (transfers to/from blocked addresses revert).
contract MockUSDC {
    string public constant name = "Mock USDC";
    string public constant symbol = "USDC";
    uint8 public constant decimals = 6;

    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;
    mapping(address => bool) public blocked;

    event Transfer(address indexed from, address indexed to, uint256 value);

    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
        totalSupply += amount;
    }

    function setBlocked(address who, bool isBlocked) external {
        blocked[who] = isBlocked;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        return true;
    }

    function transfer(address to, uint256 amount) external virtual returns (bool) {
        _move(msg.sender, to, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external virtual returns (bool) {
        allowance[from][msg.sender] -= amount;
        _move(from, to, amount);
        return true;
    }

    function _move(address from, address to, uint256 amount) internal virtual {
        require(!blocked[from] && !blocked[to], "blocked");
        balanceOf[from] -= amount;
        balanceOf[to] += amount;
        emit Transfer(from, to, amount);
    }
}

/// @dev Takes a 1% fee on every transfer (burned).
contract FeeOnTransferToken is MockUSDC {
    function _move(address from, address to, uint256 amount) internal override {
        uint256 fee = amount / 100;
        balanceOf[from] -= amount;
        balanceOf[to] += amount - fee;
        totalSupply -= fee;
    }
}

/// @dev Returns false instead of reverting, and moves nothing, when `failing` is set.
contract FalseReturningToken is MockUSDC {
    bool public failing;

    function setFailing(bool f) external {
        failing = f;
    }

    function transfer(address to, uint256 amount) external override returns (bool) {
        if (failing) return false;
        _move(msg.sender, to, amount);
        return true;
    }

    function transferFrom(address from, address to, uint256 amount) external override returns (bool) {
        if (failing) return false;
        allowance[from][msg.sender] -= amount;
        _move(from, to, amount);
        return true;
    }
}

/// @dev Returns true from transferFrom without moving anything.
contract LyingToken is MockUSDC {
    function transferFrom(address, address, uint256) external pure override returns (bool) {
        return true;
    }
}

/// @dev Calls back into the escrow during transfers with a preset payload.
contract ReentrantToken is MockUSDC {
    address public target;
    bytes public payload;
    bool public lastCallOk;
    bytes public lastCallReturn;

    function arm(address target_, bytes calldata payload_) external {
        target = target_;
        payload = payload_;
    }

    function _move(address from, address to, uint256 amount) internal override {
        super._move(from, to, amount);
        if (target != address(0)) {
            address t = target;
            target = address(0);
            (lastCallOk, lastCallReturn) = t.call(payload);
        }
    }
}
