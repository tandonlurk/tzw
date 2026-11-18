// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import {ChallengeEscrow} from "./ChallengeEscrow.sol";

/// @title AgentVault
/// @notice Holds one agent's USDC on behalf of its owner. The owner (a user's own wallet) funds it,
///         sets its limits and can withdraw at any time. The operator (whoever runs the agent) can do
///         exactly one thing with the funds: stake them into a `ChallengeEscrow` challenge that names
///         this vault as a party, within the owner's per-challenge and total-exposure caps.
/// @dev The operator can never send funds to any address. Escrow payouts and refunds come back to this
///      vault because the vault is the party. A compromised operator can at worst commit up to
///      `maxExposure` into challenges the resolver has registered with this vault as a party; it needs
///      the resolver as well to steer an outcome. The owner can pause or replace the operator at any time.
///
///      Same token rules as ChallengeEscrow: ERC-20 interface only (6 decimals), no payable functions.
contract AgentVault {
    address public immutable USDC;
    ChallengeEscrow public immutable ESCROW;
    address public immutable OWNER;

    address public operator;
    bool public paused;
    /// @notice Largest stake the operator may commit to one challenge.
    uint256 public maxPerChallenge;
    /// @notice Largest total the operator may have committed across open challenges.
    uint256 public maxExposure;
    /// @notice Sum of stakes committed and not yet released.
    uint256 public exposure;
    /// @notice Stake committed per challenge id, zero once released (or never staked).
    mapping(bytes32 => uint256) public staked;

    event OperatorSet(address indexed operator);
    event LimitsSet(uint256 maxPerChallenge, uint256 maxExposure);
    event PausedSet(bool paused);
    event Staked(bytes32 indexed id, uint256 amount);
    event Released(bytes32 indexed id, uint256 amount, ChallengeEscrow.Status status);
    event Withdrawn(address indexed to, uint256 amount);

    error ZeroAddress();
    error NotOwner();
    error NotOperator();
    error IsPaused();
    error AlreadyStaked();
    error NotStaked();
    error NotAParty();
    error WrongStatus(ChallengeEscrow.Status status);
    error OverPerChallenge(uint256 stake, uint256 max);
    error OverExposure(uint256 exposureAfter, uint256 max);
    error TransferFailed();

    modifier onlyOwner() {
        if (msg.sender != OWNER) revert NotOwner();
        _;
    }

    constructor(
        address usdc,
        address escrow,
        address owner_,
        address operator_,
        uint256 maxPerChallenge_,
        uint256 maxExposure_
    ) {
        if (usdc == address(0) || escrow == address(0) || owner_ == address(0)) {
            revert ZeroAddress();
        }
        USDC = usdc;
        ESCROW = ChallengeEscrow(escrow);
        OWNER = owner_;
        operator = operator_;
        maxPerChallenge = maxPerChallenge_;
        maxExposure = maxExposure_;
        emit OperatorSet(operator_);
        emit LimitsSet(maxPerChallenge_, maxExposure_);
    }

    // ------------------------------------------------------------------ operator

    /// @notice Deposit this vault's stake into escrow challenge `id`. Operator only.
    /// @dev Checks, in order: not paused; not already staked; the challenge is `Created` and names this
    ///      vault; the stake is within both caps. The escrow then pulls exactly `stake`.
    function stake(bytes32 id) external {
        if (msg.sender != operator) revert NotOperator();
        if (paused) revert IsPaused();
        if (staked[id] != 0) revert AlreadyStaked();
        ChallengeEscrow.Challenge memory c = ESCROW.getChallenge(id);
        if (c.status != ChallengeEscrow.Status.Created) revert WrongStatus(c.status);
        if (c.partyA != address(this) && c.partyB != address(this)) revert NotAParty();
        if (c.stake > maxPerChallenge) revert OverPerChallenge(c.stake, maxPerChallenge);
        uint256 after_ = exposure + c.stake;
        if (after_ > maxExposure) revert OverExposure(after_, maxExposure);

        staked[id] = c.stake;
        exposure = after_;
        _approve(address(ESCROW), c.stake);
        ESCROW.deposit(id);
        _approve(address(ESCROW), 0);
        emit Staked(id, c.stake);
    }

    // ------------------------------------------------------------------ anyone

    /// @notice Free the exposure of a finished challenge. Anyone may call; it moves no funds.
    function release(bytes32 id) external {
        uint256 amount = staked[id];
        if (amount == 0) revert NotStaked();
        ChallengeEscrow.Status s = ESCROW.getChallenge(id).status;
        if (
            s != ChallengeEscrow.Status.Resolved && s != ChallengeEscrow.Status.Voided
                && s != ChallengeEscrow.Status.Refunded
        ) revert WrongStatus(s);
        staked[id] = 0;
        exposure -= amount;
        emit Released(id, amount, s);
    }

    // ------------------------------------------------------------------ owner

    function withdraw(address to, uint256 amount) external onlyOwner {
        if (to == address(0)) revert ZeroAddress();
        (bool ok, bytes memory ret) = USDC.call(abi.encodeWithSelector(0xa9059cbb, to, amount)); // transfer
        if (!ok || (ret.length != 0 && (ret.length < 32 || !abi.decode(ret, (bool))))) revert TransferFailed();
        emit Withdrawn(to, amount);
    }

    /// @notice Replace the operator, or set it to zero to revoke it entirely.
    function setOperator(address operator_) external onlyOwner {
        operator = operator_;
        emit OperatorSet(operator_);
    }

    function setLimits(uint256 maxPerChallenge_, uint256 maxExposure_) external onlyOwner {
        maxPerChallenge = maxPerChallenge_;
        maxExposure = maxExposure_;
        emit LimitsSet(maxPerChallenge_, maxExposure_);
    }

    function setPaused(bool paused_) external onlyOwner {
        paused = paused_;
        emit PausedSet(paused_);
    }

    // ------------------------------------------------------------------ views

    function balance() public view returns (uint256) {
        (bool ok, bytes memory ret) = USDC.staticcall(abi.encodeWithSelector(0x70a08231, address(this))); // balanceOf
        if (!ok || ret.length < 32) revert TransferFailed();
        return abi.decode(ret, (uint256));
    }

    function _approve(address spender, uint256 amount) private {
        (bool ok, bytes memory ret) = USDC.call(abi.encodeWithSelector(0x095ea7b3, spender, amount)); // approve
        if (!ok || (ret.length != 0 && (ret.length < 32 || !abi.decode(ret, (bool))))) revert TransferFailed();
    }
}

/// @title VaultFactory
/// @notice Deploys AgentVaults owned by the caller. `agentRef` ties a vault to an agent id off-chain;
///         the factory does not interpret it.
contract VaultFactory {
    address public immutable USDC;
    address public immutable ESCROW;
    mapping(address => bool) public isVault;

    event VaultCreated(
        address indexed vault,
        address indexed owner,
        address indexed operator,
        bytes32 agentRef,
        uint256 maxPerChallenge,
        uint256 maxExposure
    );

    error ZeroAddress();

    constructor(address usdc, address escrow) {
        if (usdc == address(0) || escrow == address(0)) revert ZeroAddress();
        USDC = usdc;
        ESCROW = escrow;
    }

    function createVault(address operator, uint256 maxPerChallenge, uint256 maxExposure, bytes32 agentRef)
        external
        returns (AgentVault vault)
    {
        vault = new AgentVault(USDC, ESCROW, msg.sender, operator, maxPerChallenge, maxExposure);
        isVault[address(vault)] = true;
        emit VaultCreated(address(vault), msg.sender, operator, agentRef, maxPerChallenge, maxExposure);
    }
}
