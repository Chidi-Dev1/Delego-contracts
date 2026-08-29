# Delego Smart Contracts

<div align="center">

**Soroban smart contracts powering [Delego](https://github.com/DelegoLabs/Delego) — AI-Powered Delegated Commerce on Stellar**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/Rust-1.70-orange)](https://www.rust-lang.org/)
[![Soroban SDK 22](https://img.shields.io/badge/Soroban%20SDK-22.0.0-blue)](https://soroban.stellar.org/)

</div>

## 🌟 Overview

This repository contains the trust-critical Soroban smart contracts for the Delego platform. They handle escrow management, spending permissions, and delegation registry operations on the Stellar blockchain — the blockchain layer for secure, trust-minimized agent-mediated commerce.

### 🏗️ Repository Map

Delego is split across three repositories:

| Repository | Purpose |
|---|---|
| [Delego](https://github.com/DelegoLabs/Delego) | Frontend web application |
| [Delego-backend](https://github.com/DelegoLabs/Delego-backend) | Backend microservices, agents, shared SDK/types |
| [Delego-contracts](https://github.com/DelegoLabs/Delego-contracts) | **This repo** — Soroban smart contracts |

```
Delego (web)  ──>  Delego-backend (API/gateway)  ──>  Soroban RPC  ──>  These contracts
```

### Design Principles

- **Security First**: All contracts undergo rigorous security audits
- **Gas Efficiency**: Optimized for minimal gas consumption
- **Upgradability**: Designed with upgrade patterns in mind
- **Auditability**: Clear, well-documented code
- **Test Coverage**: Comprehensive test suites (unit + cross-contract integration)

## 📦 Contracts

| Contract | Path | Purpose |
|---|---|---|
| Escrow | [`escrow/`](./escrow) | Locks funds during purchases, time-locked release, refunds, yield, disputes, multi-admin |
| Permissions | [`permissions/`](./permissions) | Delegated spending authority, allowances, per-tx limits, relayed (gasless) spends, multi-owner grants |
| Delegation Registry | [`delegation_registry/`](./delegation_registry) | Tracks delegations, expiry, versioned rollback/upgrade |
| Cross-contract tests | [`tests/`](./tests) | End-to-end delegated purchase flows across escrow + permissions |

Each contract is a Cargo workspace member. Unit tests live as `#[cfg(test)]` modules in `*/src/test.rs`, contract-level integration tests as modules in `*/src/integration_tests.rs`, and cross-contract integration tests in the root `tests/` package.

## 🛠️ Prerequisites

- **Rust**: `>= 1.70`
  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
  ```
- **WASM target** (required to build contract `.wasm` artifacts):
  ```bash
  rustup target add wasm32-unknown-unknown
  ```
- **Soroban CLI** (for deployment and interaction):
  ```bash
  cargo install soroban-cli
  ```

## 🧪 Testing

```bash
# Run the full workspace test suite (unit + cross-contract integration)
cargo test --workspace

# Run tests with output
cargo test -- --nocapture

# Run a single test
cargo test test_function_name

# Run tests in release mode
cargo test --release
```

The cross-contract integration tests (`tests/cross_contract.rs`) exercise a complete delegated purchase: grant permission, fund escrow, spend within limits, and release.

## 🔨 Building

```bash
# Build all contracts for the WASM target
cargo build --target wasm32-unknown-unknown --release

# Build a specific contract
cargo build -p delego-escrow --target wasm32-unknown-unknown --release
```

Build artifacts land in `target/wasm32-unknown-unknown/release/`.

## 🛠️ Tooling

All five contract crates expose the same script set via `package.json`
(`build-wasm`, `test`, `lint`) and via a root [`justfile`](./justfile), so
building, testing, and linting any contract is a single command:

```bash
# Inside any contract directory
npm run build-wasm   # cargo build --target wasm32-unknown-unknown --release
npm test             # cargo test
npm run lint         # cargo clippy --all-targets -- -D warnings

# Or from the repo root with just installed
just build-wasm
just test
just lint
```

## 🚀 Deployment

### Testnet

```bash
soroban network add --global futurenet \
  --rpc-url https://rpc-futurenet.stellar.org \
  --network-passphrase "Test SDF Future Network ; September 2022"

soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/delego_escrow.wasm \
  --source <DEPLOYER_ACCOUNT> \
  --network futurenet

export ESCROW_CONTRACT_ID=<CONTRACT_ID>
```

### Mainnet

```bash
soroban network add --global public \
  --rpc-url https://soroban-rpc.stellar.org \
  --network-passphrase "Public Global Stellar Network ; September 2015"

soroban contract deploy \
  --wasm target/wasm32-unknown-unknown/release/delego_escrow.wasm \
  --source <DEPLOYER_ACCOUNT> \
  --network public
```

### Deployment Checklist

- [ ] Contract tests pass
- [ ] Code reviewed by team
- [ ] Security audit completed
- [ ] Gas optimization performed
- [ ] Documentation updated
- [ ] Deployment script tested
- [ ] Rollback plan prepared

## 🔐 Security

### Best Practices

- **Access Control**: Implement proper access control
- **Input Validation**: Validate all inputs
- **Reentrancy Protection**: Protect against reentrancy attacks
- **Integer Overflow**: Use checked arithmetic (`overflow-checks = true` in release profile)
- **Randomness**: Use secure randomness sources
- **Secret Management**: Never store secrets on-chain

### Audits

All contracts must undergo:
1. Internal code review
2. External security audit
3. Penetration testing
4. Bug bounty program

## 📚 Documentation

- [Architecture notes](./docs/architecture/contracts.md)
- Generate API docs with `cargo doc --open`

## 🤝 Contributing

When contributing to smart contracts:

1. Follow Rust best practices
2. Write comprehensive tests for every public function
3. Document all public functions
4. Consider gas efficiency
5. Security review required
6. Update documentation

See [CONTRIBUTING.md](./CONTRIBUTING.md) for general guidelines.

## Resources

- [Soroban Documentation](https://soroban.stellar.org/docs)
- [Soroban SDK](https://docs.rs/soroban-sdk/)
- [Stellar Documentation](https://developers.stellar.org/)
- [Rust Book](https://doc.rust-lang.org/book/)

## Troubleshooting

**Build Errors**
```bash
cargo clean
cargo build --target wasm32-unknown-unknown --release
```

**Test Failures**
```bash
cargo test -- --nocapture
cargo test test_function_name
```

**Deployment Issues**
```bash
soroban network inspect
soroban contract inspect --wasm contract.wasm
```

---

**Last Updated**: August 2026
