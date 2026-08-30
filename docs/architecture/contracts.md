# Smart Contract Architecture

Delego uses Soroban smart contracts to anchor trust-critical state on the Stellar blockchain, ensuring security, transparency, and programmable trust for agent-mediated commerce.

## 📋 Table of Contents

- [Overview](#overview)
- [Contract Types](#contract-types)
- [On-Chain vs Off-Chain](#on-chain-vs-off-chain)
- [Contract Interactions](#contract-interactions)
- [State Management](#state-management)
- [Upgrade Patterns](#upgrade-patterns)
- [Security Considerations](#security-considerations)

## Overview

Smart contracts are used for trust-critical operations that require blockchain guarantees, while off-chain services handle high-throughput operations like catalog search and product discovery.

### Design Principles

- **Trust-Critical On-Chain**: Only trust-critical state on-chain
- **Off-Chain Efficiency**: High-throughput operations off-chain
- **Minimal Gas**: Optimize for minimal gas usage
- **Upgradeability**: Design for contract upgrades
- **Security First**: Prioritize security in all contracts

## Contract Types

### Escrow Contract

**On-chain State**: Locked funds per order

#### Purpose

The escrow contract holds funds in trust during agent-mediated purchases, releasing funds only when predefined conditions are met.

#### Key Functions

- `create(escrow_id, buyer, seller, token)`: Create an unfunded escrow record in `Created` status
- `deposit(...)` / `fund(...)`: Lock buyer funds for an order
- `release(escrow_id)` / `partial_release(...)`: Transfer remaining/partial balance to seller
- `refund(escrow_id)`: Return funds to buyer (after timeout if needed)
- `dispute(escrow_id)` / `resolve_dispute(...)` / `resolve_dispute_quorum(...)`: Dispute lifecycle
- `cancel(escrow_id)`: Merchant cancels an unfunded escrow
- `get_escrow(escrow_id)`: Get full escrow record
- `get_receipt(escrow_id)` / `get_merchant_receipt(...)`: Buyer/seller receipts
- `get_release_eligibility(...)` / `get_refund_eligibility(...)` / `get_timeout_view(...)`: Read-only eligibility checks
- Admin: `set_limits`, `update_fee`, `add_token`, `set_create_paused`, `propose_admin`, `accept_admin`, `add_co_admin`

#### State

```rust
struct EscrowRecord {
    escrow_id: BytesN<32>,
    buyer: Address,
    seller: Address,
    token: Address,
    amount: i128,
    fee_bps: u32,
    status: EscrowStatus,
    timeout_ledger: u32,
    dispute_reason: Option<Symbol>,
    create_paused: bool,
}

enum EscrowStatus {
    Created,
    Funded,
    Released,
    Refunded,
    Cancelled,
    Disputed,
}
```

#### Use Cases

- Buyer approves purchase → funds locked in escrow
- Delivery confirmed → funds released to merchant
- Delivery failed → funds refunded to buyer
- Dispute → funds held until resolution (admin or arbiter quorum)

### Permissions Contract

**On-chain State**: Delegate spending limits

#### Purpose

The permissions contract manages delegated spending authority, allowing users to grant agents limited permission to spend on their behalf.

#### Key Functions

- `grant(owner, delegate, ...)`: Grant spending permission
- `grant_child(owner, delegate, ...)`: Derive a nested permission from an existing grant
- `revoke(owner, delegate)`: Revoke spending permission
- `transfer_permission(owner, ...)`: Transfer a permission to another account
- `can_spend(owner, delegate, amount)`: Check if amount is within limit
- `execute_spend(owner, delegate, ...)`: Spend within limits (emits `PermissionSpendEvent`)
- `get_permission(owner, delegate)`: Get permission details
- `increase_allowance(...)` / `decrease_allowance(...)`: Adjust spending limit
- `renew_permission(...)` / `update_expiry(...)`: Manage expiry
- `execute_spend_via_relayer(...)`: Gasless spend via relayer signature
- `grant_multi_owner(...)`: Multi-owner (quorum) grants
- `pause(...)` / `resume(...)` / `pause_grants(...)`: Pause controls
- `set_admin(...)` / `propose_admin(...)` / `accept_admin(...)`: Admin management

#### State

```rust
struct PermissionRecord {
    delegate: Address,
    limit_per_transaction: i128,
    limit_total: i128,
    used: i128,
    expiry: u64,
    status: PermissionStatus,
}
```

#### Use Cases

- User creates delegation → permission granted to agent
- Agent attempts payment → permission checked (`can_spend`)
- Spending limit reached → payment blocked
- User revokes delegation → permission revoked

### Delegation Registry Contract

**On-chain State**: Delegation records

#### Purpose

Tracks delegation records with expiry and versioned rollback/upgrade support.

#### Key Functions

- Register and update delegation records
- Read delegation state for off-chain services
- Versioned rollback of delegation state

### Reputation Contract

**On-chain State**: Cumulative scores

#### Purpose

The reputation contract tracks on-chain reputation scores for merchants and agents, enabling trust-based decision making.

- `record_transaction(merchant, amount, rating)`: Record transaction and rating
- `get_reputation(entity)`: Get reputation score

### Marketplace Contract

**On-chain State**: Merchant registry, multi-verifier verification, commission configuration, category discovery index, metadata cooldown policy

#### Purpose

The marketplace contract maintains a trusted on-chain registry of merchants, enabling discovery and verification of merchants, per-merchant commission tracking, reputation score snapshot pairing, and status lifecycle controls (suspend/unsuspend/close). Registration, profile updates, and verification are multi-signer safe: merchants self-register, a configured set of verifiers attests identity, and an admin (with two-step `propose_admin`/`accept_admin` handover) moderates.

#### Key Data Structures

```rust
struct RegisterParams {
    name: String,
    description: String,
    category: Symbol,
    image_url: String,
    metadata: Option<String>,
    required_verifications: u32,
}

struct Merchant {
    id: u64,
    owner: Option<Address>,
    name: String,
    description: String,
    category: Symbol,
    image_url: String,
    commission_rate_bps: u32,
    metadata: Option<String>,
    status: MerchantStatus,
    verified: bool,
    created_at: u64,
    updated_at: u64,
    reputation: Option<Address>,
}

struct MerchantView {
    id: u64,
    name: String,
    category: Symbol,
    commission_rate_bps: u32,
    verified: bool,
    status: MerchantStatus,
    reputation_score: Option<u32>,
}

struct VerificationPolicy {
    required: u32,      // verifications needed to become Verified
    max_verifications: u32,
}

struct Verifier {
    address: Address,
    label: Symbol,
    registered_at: u64,
}

struct CooldownConfig {
    value_seconds: u64,  // current metadata-update cooldown
    min_seconds: u64,     // 60s floor
    max_seconds: u64,     // 30-day ceiling
}
```

#### Status Model

`MerchantStatus` is an explicit `#[repr(u32)]` lifecycle enum:

```rust
enum MerchantStatus {
    Registered = 0, // Created, not yet verified
    Verified = 1,   // Passed the verification threshold
    Suspended = 2,  // Temporarily disabled (admin action / review)
    Closed = 3,     // Permanently removed
}
```

Transitions are enforced by helpers (`check_not_frozen_or_closed`) so that suspended/closed merchants cannot be modified, re-verified, or have commissions changed. Unsuspending restores `Verified` or `Registered` depending on the `verified` flag.

#### Key Functions

- `register_merchant(merchant, params)`: Self-register a merchant; derives `RegisterParams`, assigns the next monotonic id, builds the `Merchant` record, and indexes it in `MerchantIds` and `CategoryIndex`
- `is_name_available(name)`: Check a merchant name is not already claimed
- `update_merchant_profile(...)` / `update_metadata(...)`: Owner/admin updates; metadata writes for non-admins are gated by the cooldown policy (`MetadataLockActive`)
- `verify_merchant(merchant_id, verifier)`: Registered verifier attests a merchant; when `VerifiedCount` reaches the policy's `required` threshold the merchant flips to `Verified`
- `revoke_verification(admin, merchant_id)`: Admin clears verification state and resets `VerifiedCount`/verifier list
- `add_verifier(...)` / `remove_verifier(...)`: Admin manages the verifier set; removal is rejected if it would strand an existing policy (`required > remaining verifiers`)
- `get_merchant(merchant_id)` / `get_merchant_view(merchant_id)`: Full record vs. discovery view; the view injects a `reputation_score` snapshot by cross-contract calling the paired reputation contract (`get_reputation`)
- `get_merchants(offset, limit)` / `get_merchants_by_category(category, offset, limit)`: Paginated discovery over `MerchantIds` / `CategoryIndex` (page size capped at 50)
- `set_merchant_commission(...)` / `get_commission(...)`: Per-merchant commission in basis points (≤ 10_000)
- `suspend_merchant(...)` / `unsuspend_merchant(...)` / `close_merchant(...)`: Admin moderation lifecycle
- `set_merchant_reputation(...)` / `set_reputation_contract(...)`: Pair a merchant (or the whole registry) with a reputation contract for score injection
- `propose_admin(...)` / `accept_admin(...)`: Two-step admin handover
- `set_metadata_cooldown(...)` / `get_metadata_cooldown(...)`: Configure the metadata update cooldown, clamped to `[60s, 30d]` (default 24h)
- `version()`: Returns contract name and semver (`0.2.0`)

#### State (Storage Keys)

- Instance: `Admin`, `PendingAdmin`, `NextMerchantId`, `Verifiers`, `MetadataCooldown`/`MetadataCooldownConfig`, `GlobalReputationContract`
- Persistent per merchant: `Merchant(id)`, `MerchantName(name)`, `FreedName(name)`, `ArchivedMerchant(id)`, `VerifiedCount(id)`, `VerificationPolicy(id)`, `MerchantVerifier(id, verifier)`, `MerchantVerifierList(id)`, `LastMetadataUpdate(id)`
- Persistent indexes: `MerchantIds` (all ids), `CategoryIndex(category)` (ids per category)

#### CategoryIndex & Discovery

`CategoryIndex` maps a `Symbol` category to a `Vec<u64>` of merchant ids, appended on registration and read with offset/limit pagination so off-chain services can render category-filtered storefronts without scanning every merchant. TTL for all persistent entries is extended on access/creation (`~30 days` of ledgers).

#### Cooldown Policy

Metadata updates are rate-limited to prevent squatting/abuse: a non-admin owner may only update `metadata` once per cooldown window (default 24 hours, configurable between 60 seconds and 30 days). Admin updates bypass the cooldown. Exceeding it returns `MetadataLockActive`.

#### Use Cases

- Merchant registers with name/category/commission intent → `Registered`
- Registered verifiers attest identity → threshold reached → `Verified`
- Storefront/catalog services page through `get_merchants_by_category`
- Merchant misconduct → `Suspended`; repeat offense → `Closed` (permanently removed from discovery)

## On-Chain vs Off-Chain

### On-Chain (Smart Contracts)

Trust-critical operations that require blockchain guarantees:

- **Escrow**: Fund locking and release
- **Permissions**: Spending authority delegation
- **Delegation Registry**: Delegation records with expiry and rollback
- **Reputation**: Reputation score tracking
- **Marketplace**: Merchant registry, verification, and discovery

### Off-Chain (Services)

High-throughput operations that don't require blockchain guarantees:

- **Catalog**: Product catalog and search
- **Search**: Product search and comparison
- **Analytics**: Spending analytics and reporting
- **Notifications**: Email and push notifications

### Hybrid Approach

Some operations use a hybrid approach:

- **Order Creation**: Off-chain order creation, on-chain escrow
- **Payment**: Off-chain payment initiation, on-chain settlement
- **Reputation**: Off-chain rating collection, on-chain aggregation

## Contract Interactions

### Cross-Contract Calls

Contracts can call other contracts:

```rust
// Escrow contract calling Permissions contract
let allowed = permissions::can_spend(
    &e,
    &owner,
    &delegate,
    &amount
);
```

### Contract-to-Service Communication

Services interact with contracts via the wallet service:

```
Wallet Service
    ↓
Soroban RPC
    ↓
Smart Contracts
```

### Event Emission

Contracts emit events for off-chain services:

```rust
// Emit event when funds are locked
events::publish(
    &e,
    (Symbol::new(&e, Symbol::short("locked")), order_id, amount)
);
```

## State Management

### Persistent Storage

Contract state is stored in persistent Soroban storage:

```rust
// Store permission
e.storage().persistent().set(
    &StorageKey::from(b"permission"),
    &permission
);

// Retrieve permission
let permission: Permission = e.storage()
    .persistent()
    .get(&StorageKey::from(b"permission"))
    .unwrap();
```

### Temporary Storage

Temporary storage for ephemeral data:

```rust
// Store temporary data
e.storage().temporary().set(
    &StorageKey::from(b"temp"),
    &data
);
```

### Instance Storage

Instance storage for contract instances:

```rust
// Store instance data
e.storage().instance().set(
    &StorageKey::from(b"instance"),
    &data
);
```

## Upgrade Patterns

### Upgradeable Contracts

Contracts are designed to be upgradeable:

```rust
// Check if upgrade is authorized
require!(
    e.storage().instance().has(&StorageKey::from(b"upgrade_authority")),
    "Not authorized"
);

// Upgrade contract
e.deployer()
    .update_current_contract_wasm(new_wasm);
```

### Migration Strategy

When upgrading contracts:

1. Deploy new contract
2. Migrate state from old contract
3. Update references
4. Decommission old contract

### Versioning

Contracts include version information:

```rust
struct ContractInfo {
    version: u32,
    name: String,
    upgraded_at: u64,
}
```

## Security Considerations

### Error Code Allocation

Cross-contract bridges surface numeric `u32` error codes from different contracts. To keep unified error mapping unambiguous, each contract's error enum owns a disjoint numeric range. The allocation table below is normative and is enforced by a repo-level unit test.

| Contract | Error enum | Allocated numeric range |
|----------|------------|-------------------------|
| Escrow | `EscrowError` | `1000..=1999` |
| Permissions | `PermissionError` | `2000..=2999` |
| Delegation Registry | `DelegationError` | `3000..=3999` |
| Reputation | `ReputationError` | `4000..=4999` |
| Marketplace | `MarketplaceError` | `5000..=5999` |

Within a contract, error discriminants must stay inside the allocated range. New error codes require updating the contract enum; if a range is exhausted, extend the allocation table before adding another range.

### Access Control

Contracts implement strict access control:

```rust
// Only owner can call this function
require!(
    e.invoker() == owner,
    "Not authorized"
);
```

### Input Validation

All inputs are validated:

```rust
// Validate amount is positive
require!(
    amount > 0,
    "Amount must be positive"
);
```

### Reentrancy Protection

Contracts protect against reentrancy:

```rust
// Reentrancy guard
let guard = ReentrancyGuard::new(&e);
guard.enter();
// ... contract logic
guard.exit();
```

### Overflow Protection

Contracts protect against overflow:

```rust
// Use checked arithmetic
let new_amount = amount.checked_add(spent).unwrap();
```

### Audit Trail

All contract operations are logged:

```rust
// Log operation
events::publish(
    &e,
    (Symbol::new(&e, Symbol::short("operation")), operation_id, details)
);
```

## Gas Optimization

### Efficient Storage

Optimize storage for minimal gas usage:

```rust
// Use compact data structures
struct CompactPermission {
    delegator: Address,
    delegate: Address,
    limit: i128,  // Use i128 instead of u256
    expiry: u64,
}
```

### Batch Operations

Batch operations to reduce gas:

```rust
// Batch multiple operations
for permission in permissions {
    check_permission(&e, &permission);
}
```

### Lazy Evaluation

Defer expensive operations:

```rust
// Only compute when needed
if needs_computation {
    compute_expensive_operation();
}
```

## Testing

### Unit Tests

Test individual contract functions:

```rust
#[test]
fn test_lock_funds() {
    let env = Env::default();
    let contract_id = env.register_contract(None, EscrowContract);
    let client = EscrowContractClient::new(&env, &contract_id);

    client.lock_funds(&env, &order_id, &amount, &buyer, &merchant);
    
    let balance = client.get_balance(&env, &order_id);
    assert_eq!(balance, amount);
}
```

### Integration Tests

Test contract interactions:

```rust
#[test]
fn test_escrow_permissions_integration() {
    let env = Env::default();
    // Test interaction between escrow and permissions contracts
}
```

### Fuzzing

Use fuzzing to find edge cases:

```rust
#[test]
fn fuzz_lock_funds() {
    // Fuzz test with random inputs
}
```

## Deployment

### Testnet Deployment

Deploy contracts to the Stellar testnet:

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/delego_escrow.wasm \
  --source <DEPLOYER_ADDRESS> \
  --network testnet
```

### Mainnet Deployment

Deploy contracts to Stellar mainnet:

```bash
soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/delego_escrow.wasm \
  --source <DEPLOYER_ADDRESS> \
  --network public
```

### Verification

Verify contract deployment:

```bash
soroban contract inspect \
  --id <contract-id> \
  --network testnet
```

## Monitoring

### Contract Events

Monitor contract events:

```bash
soroban contract events \
  --contract-id <contract-id> \
  --network testnet
```

### State Queries

Query contract state:

```bash
soroban contract invoke \
  --id <contract-id> \
  --fn get_escrow \
  --arg <order-id> \
  --network testnet
```

### Analytics

Track contract analytics:

- Transaction volume
- Gas usage
- Error rates
- Active contracts

## Documentation

See the repository [README.md](../../README.md) for detailed contract documentation including:

- Contract implementation details
- Development setup
- Testing procedures
- Deployment guides
- Security best practices

---

**Last Updated**: August 2026
