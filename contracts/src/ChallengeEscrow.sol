// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/// @title ChallengeEscrow
/// @notice Escrow for two-party, equal-stake challenges settled in a single ERC-20 (USDC, 6 decimals).
/// @dev Resolution is administered by the `resolver` address (the backend). It is NOT trustless:
///      the resolver decides the winner. What the contract guarantees is narrower:
///      - funds for a challenge can only ever go to that challenge's two parties;
///      - nobody (including the resolver) can withdraw to a third address;
///      - if the resolver disappears, anyone can refund both stakes after `resolveDeadline + GRACE`.
///
///      On Arc the stake token is also the native gas token. This contract only uses the ERC-20
///      interface: it has no payable function, no receive/fallback, and never reads `msg.value`.
contract ChallengeEscrow {
    enum Status {
        None, // id unused
        Created, // waiting for deposits
        Active, // both stakes locked
        Resolved, // terminal: winner credited 2x stake
        Voided, // terminal: resolver returned stakes (draw / unresolvable / cancelled)
        Refunded // terminal: timeout refund
    }

    struct Challenge {
        address partyA;
        uint64 fundDeadline;
        Status status;
        bool depositedA;
        bool depositedB;
        address partyB;
        uint64 resolveDeadline;
        uint256 stake;
        address winner; // zero unless Resolved
    }

    /// @notice Time after `resolveDeadline` before an unresolved Active challenge can be refunded by anyone.
    uint64 public constant GRACE = 1 hours;

    /// @notice Stake token. Immutable.
    address public immutable USDC;
    /// @notice Only address allowed to create, resolve and void challenges. Immutable (no transfer).
    address public immutable RESOLVER;

    /// @notice Tokens this contract owes: locked stakes plus deferred payouts. Equals its token
    ///         balance unless someone transfers tokens to it directly.
    uint256 public totalHeld;

    /// @notice Payouts whose push transfer failed (e.g. recipient blocklisted by the token), by
    ///         challenge and party. Collect with `claim`.
    mapping(bytes32 id => mapping(address party => uint256 amount)) public owed;

    mapping(bytes32 id => Challenge) private _challenges;
    uint256 private _lock = 1;

    event Created(
        bytes32 indexed id,
        address indexed partyA,
        address indexed partyB,
        uint256 stake,
        uint64 fundDeadline,
        uint64 resolveDeadline
    );
    event Deposited(bytes32 indexed id, address indexed party, uint256 amount);
    event Activated(bytes32 indexed id, address indexed partyA, address indexed partyB, uint256 totalStaked);
    event Resolved(bytes32 indexed id, address indexed winner, address indexed loser, uint256 payout);
    event Voided(bytes32 indexed id, address indexed partyA, address indexed partyB, uint256 refundA, uint256 refundB);
    event Refunded(
        bytes32 indexed id, address indexed partyA, address indexed partyB, uint256 refundA, uint256 refundB
    );
    /// @notice A payout could not be pushed and is now held in `owed` for `party`.
    event PayoutDeferred(bytes32 indexed id, address indexed party, uint256 amount);
    event Claimed(bytes32 indexed id, address indexed party, uint256 amount);

    error ZeroAddress();
    error NotResolver();
    error Reentrancy();
    error ChallengeExists();
    error ZeroStake();
    error InvalidParties();
    error InvalidDeadlines();
    error WrongStatus(Status current);
    error NotParty();
    error AlreadyDeposited();
    error FundingClosed();
    error RefundNotAvailable();
    error NothingOwed();
    error TransferFailed();
    error UnexpectedTransferAmount(uint256 expected, uint256 received);

    modifier onlyResolver() {
        if (msg.sender != RESOLVER) revert NotResolver();
        _;
    }

    modifier nonReentrant() {
        if (_lock != 1) revert Reentrancy();
        _lock = 2;
        _;
        _lock = 1;
    }

    constructor(address usdc, address resolver) {
        if (usdc == address(0) || resolver == address(0)) revert ZeroAddress();
        USDC = usdc;
        RESOLVER = resolver;
    }

    /// @notice Register a challenge. Resolver only, so nobody can spam challenges naming others.
    ///         Creating moves no funds; each party consents by calling `deposit` itself.
    /// @param id Caller-chosen unique id (the backend's challenge id).
    /// @param stake Amount each party must deposit, in token base units.
    /// @param fundDeadline Deposits are accepted while `block.timestamp < fundDeadline`.
    /// @param resolveDeadline Expected resolution time. Must be after `fundDeadline`.
    function create(
        bytes32 id,
        address partyA,
        address partyB,
        uint256 stake,
        uint64 fundDeadline,
        uint64 resolveDeadline
    ) external onlyResolver {
        if (_challenges[id].status != Status.None) revert ChallengeExists();
        if (stake == 0) revert ZeroStake();
        if (partyA == address(0) || partyB == address(0) || partyA == partyB) revert InvalidParties();
        if (partyA == address(this) || partyB == address(this)) revert InvalidParties();
        if (fundDeadline <= block.timestamp || resolveDeadline <= fundDeadline) revert InvalidDeadlines();

        _challenges[id] = Challenge({
            partyA: partyA,
            fundDeadline: fundDeadline,
            status: Status.Created,
            depositedA: false,
            depositedB: false,
            partyB: partyB,
            resolveDeadline: resolveDeadline,
            stake: stake,
            winner: address(0)
        });

        emit Created(id, partyA, partyB, stake, fundDeadline, resolveDeadline);
    }

    /// @notice Deposit the caller's stake. Caller must be a party, must not have deposited, and must
    ///         have approved this contract for `stake`. The second deposit activates the challenge.
    /// @dev Reverts unless the contract's balance rises by exactly `stake`, so fee-on-transfer or
    ///      otherwise non-standard tokens cannot corrupt the accounting.
    function deposit(bytes32 id) external nonReentrant {
        Challenge storage c = _challenges[id];
        if (c.status != Status.Created) revert WrongStatus(c.status);
        if (block.timestamp >= c.fundDeadline) revert FundingClosed();

        if (msg.sender == c.partyA) {
            if (c.depositedA) revert AlreadyDeposited();
            c.depositedA = true;
        } else if (msg.sender == c.partyB) {
            if (c.depositedB) revert AlreadyDeposited();
            c.depositedB = true;
        } else {
            revert NotParty();
        }

        uint256 stake = c.stake;
        bool activated = c.depositedA && c.depositedB;
        if (activated) c.status = Status.Active;
        totalHeld += stake;

        uint256 balanceBefore = _balance();
        _pull(msg.sender, stake);
        uint256 received = _balance() - balanceBefore;
        if (received != stake) revert UnexpectedTransferAmount(stake, received);

        emit Deposited(id, msg.sender, stake);
        if (activated) emit Activated(id, c.partyA, c.partyB, stake * 2);
    }

    /// @notice Declare the winner of an Active challenge and pay it both stakes. Resolver only.
    /// @dev Allowed at any time while Active, before or after `resolveDeadline`, until someone has
    ///      used the timeout `refund`. The contract cannot check the outcome; it trusts the resolver.
    function resolve(bytes32 id, address winner) external onlyResolver nonReentrant {
        Challenge storage c = _challenges[id];
        if (c.status != Status.Active) revert WrongStatus(c.status);
        if (winner != c.partyA && winner != c.partyB) revert NotParty();

        c.status = Status.Resolved;
        c.winner = winner;
        uint256 payout = c.stake * 2;
        address loser = winner == c.partyA ? c.partyB : c.partyA;

        emit Resolved(id, winner, loser, payout);
        _pay(id, winner, payout);
    }

    /// @notice Void a challenge and return whatever each party deposited. Resolver only.
    /// @dev Works while Created (cancel before funding completes) or Active (draw / unresolvable).
    function resolveVoid(bytes32 id) external onlyResolver nonReentrant {
        Challenge storage c = _challenges[id];
        if (c.status != Status.Created && c.status != Status.Active) revert WrongStatus(c.status);

        c.status = Status.Voided;
        (uint256 refundA, uint256 refundB) = _deposits(c);

        emit Voided(id, c.partyA, c.partyB, refundA, refundB);
        _returnStakes(id, c.partyA, refundA, c.partyB, refundB);
    }

    /// @notice Timeout refund, callable by anyone. Returns each deposit to its depositor when
    ///         - the challenge is not fully funded and `block.timestamp >= fundDeadline`, or
    ///         - the challenge is Active and `block.timestamp >= resolveDeadline + GRACE`.
    function refund(bytes32 id) external nonReentrant {
        Challenge storage c = _challenges[id];
        if (c.status == Status.Created) {
            if (block.timestamp < c.fundDeadline) revert RefundNotAvailable();
        } else if (c.status == Status.Active) {
            if (block.timestamp < uint256(c.resolveDeadline) + GRACE) revert RefundNotAvailable();
        } else {
            revert WrongStatus(c.status);
        }

        c.status = Status.Refunded;
        (uint256 refundA, uint256 refundB) = _deposits(c);

        emit Refunded(id, c.partyA, c.partyB, refundA, refundB);
        _returnStakes(id, c.partyA, refundA, c.partyB, refundB);
    }

    /// @notice Retry a deferred payout. Callable by anyone; tokens only ever go to `party`.
    function claim(bytes32 id, address party) external nonReentrant {
        uint256 amount = owed[id][party];
        if (amount == 0) revert NothingOwed();

        owed[id][party] = 0;
        totalHeld -= amount;

        emit Claimed(id, party, amount);
        if (!_tryTransfer(party, amount)) revert TransferFailed();
    }

    /// @notice Full stored state of a challenge. `status == None` means the id is unused.
    function getChallenge(bytes32 id) external view returns (Challenge memory) {
        return _challenges[id];
    }

    function _deposits(Challenge storage c) private view returns (uint256 a, uint256 b) {
        a = c.depositedA ? c.stake : 0;
        b = c.depositedB ? c.stake : 0;
    }

    function _returnStakes(bytes32 id, address partyA, uint256 refundA, address partyB, uint256 refundB) private {
        if (refundA != 0) _pay(id, partyA, refundA);
        if (refundB != 0) _pay(id, partyB, refundB);
    }

    /// @dev Push `amount` to `to`. If the token transfer fails (USDC blocklists addresses, and on
    ///      Arc such transfers revert) the amount is parked in `owed` instead of reverting, so one
    ///      party's failing transfer can never trap the other party's funds or block settlement.
    function _pay(bytes32 id, address to, uint256 amount) private {
        if (_tryTransfer(to, amount)) {
            totalHeld -= amount;
        } else {
            owed[id][to] += amount;
            emit PayoutDeferred(id, to, amount);
        }
    }

    function _tryTransfer(address to, uint256 amount) private returns (bool) {
        (bool ok, bytes memory ret) = USDC.call(abi.encodeWithSelector(0xa9059cbb, to, amount)); // transfer
        return ok && (ret.length == 0 || (ret.length >= 32 && abi.decode(ret, (bool))));
    }

    function _pull(address from, uint256 amount) private {
        // transferFrom
        (bool ok, bytes memory ret) = USDC.call(abi.encodeWithSelector(0x23b872dd, from, address(this), amount));
        if (!ok || (ret.length != 0 && (ret.length < 32 || !abi.decode(ret, (bool))))) revert TransferFailed();
    }

    function _balance() private view returns (uint256) {
        (bool ok, bytes memory ret) = USDC.staticcall(abi.encodeWithSelector(0x70a08231, address(this))); // balanceOf
        if (!ok || ret.length < 32) revert TransferFailed();
        return abi.decode(ret, (uint256));
    }
}
