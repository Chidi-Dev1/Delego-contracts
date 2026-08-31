//! Delego Marketplace Contract (Merchant Registry & Discovery)
//!
//! Maintains a trusted on-chain registry of merchants with multi-verifier verification,
//! paginated category and name discovery, commission configuration, metadata cooldown lock,
//! status lifecycle controls, and reputation score snapshot integration.

// Contract crates compile as no_std for release and wasm builds, but keep std
// enabled during testing so dev-dependencies and test assertions operate normally.
// This exact conditional form must be consistent across all workspace contract crates.
#![cfg_attr(not(test), no_std)]
#![allow(clippy::too_many_arguments)]
#![warn(missing_docs)]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, InvokeError,
    String, Symbol, Vec,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[contracttype]
#[repr(u32)]
pub enum MerchantStatus {
    Registered = 0, // Created, not yet verified
    Verified = 1,   // Passed verification
    Suspended = 2,  // Temporarily disabled (admin action / review)
    Closed = 3,     // Permanently removed
    All = 4,        // Filter sentinel for cursor discovery (matches every status)
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Merchant {
    pub id: u64,
    pub owner: Option<Address>,
    pub name: String,
    pub description: String,
    pub category: Symbol,
    pub image_url: String,
    pub commission_rate_bps: u32,
    pub metadata: Option<String>,
    pub status: MerchantStatus,
    pub verified: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub reputation: Option<Address>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct MerchantView {
    pub id: u64,
    pub name: String,
    pub category: Symbol,
    pub commission_rate_bps: u32,
    pub verified: bool,
    pub status: MerchantStatus,
    pub reputation_score: Option<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub enum ReputationResolution {
    NotConfigured,
    Available(u32, u64),
    CallFailed,
pub struct DiscoveryPage {
    pub items: Vec<MerchantView>,
    pub total: u32,
    pub next_offset: Option<u32>,
    pub next_cursor: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct MerchantCursor {
    pub after_id: u64,
    pub status: MerchantStatus,
    pub limit: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct MerchantViewDetailed {
    pub view: MerchantView,
    pub reputation: ReputationResolution,
pub struct CategoryEntry {
    pub key: Symbol,
    pub normalized: Symbol,
    pub display: String,
    pub added_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct NameRelease { pub name: String, pub released_at: u64, pub previous_merchant: u64 }

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct RegisterParams {
    pub name: String,
    pub description: String,
    pub category: Symbol,
    pub image_url: String,
    pub metadata: Option<String>,
    pub required_verifications: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct VerificationPolicy {
    pub required: u32,
    pub max_verifications: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct Verifier {
    pub address: Address,
    pub label: Symbol,
    pub registered_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContractVersion {
    pub name: Symbol,
    pub semver: Symbol,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MarketplaceError {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    MerchantNotFound = 4,
    AlreadyVerified = 5,
    InvalidCommissionBps = 6,
    DuplicateMerchantName = 7,
    MerchantFrozen = 8,
    MerchantClosed = 9,
    VerifierAlreadyExists = 10,
    VerifierNotFound = 11,
    InsufficientVerifications = 12,
    MetadataLockActive = 13,
    InvalidCategory = 14,
    InvalidParam = 15,
    NoPendingAdmin = 16,
    VerificationCountOverflow = 16,
}

// --- Events ---

#[contracttype]
#[derive(Clone, Debug)]
pub struct MerchantRegisteredEvent {
    pub merchant_id: u64,
    pub owner: Address,
    pub name: String,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MerchantVerifiedEvent {
    pub merchant_id: u64,
    pub verifier: Address,
    pub verified: bool,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MerchantProfileUpdatedEvent {
    pub merchant_id: u64,
    pub updated_fields: Symbol,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MerchantMetadataUpdatedEvent {
    pub merchant_id: u64,
    pub new_metadata: Option<String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CooldownConfig {
    pub value_seconds: u64,
    pub min_seconds: u64,
    pub max_seconds: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataCooldownSetEvent {
    pub previous: Option<u64>,
    pub current: u64,
    pub set_by: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MerchantCommissionSetEvent {
    pub merchant_id: u64,
    pub commission_bps: u32,
    pub set_by: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MerchantSuspendedEvent {
    pub merchant_id: u64,
    pub suspended_by: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MerchantUnsuspendedEvent {
    pub merchant_id: u64,
    pub unsuspended_by: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MerchantClosedEvent {
    pub merchant_id: u64,
    pub closed_by: Address,
    pub reason: Symbol,
    pub name: String,
    pub category: Symbol,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct VerifierAddedEvent {
    pub verifier: Address,
    pub label: Symbol,
    pub added_by: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct VerifierRemovedEvent {
    pub verifier: Address,
    pub removed_by: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MerchantVerificationRevokedEvent {
    pub merchant_id: u64,
    pub revoked_by: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminProposedEvent {
    pub current_admin: Address,
    pub new_admin: Address,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminAcceptedEvent {
    pub previous_admin: Address,
    pub new_admin: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct MerchantReputationSetEvent {
    pub merchant_id: u64,
    pub reputation: Option<Address>,
    pub set_by: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct CategoryAddedEvent {
    pub key: Symbol,
    pub normalized: Symbol,
    pub added_by: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct CategoryRemovedEvent {
    pub key: Symbol,
    pub removed_by: Address,
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MerchantPrunedEvent {
    pub pruned_count: u32,
    pub pruned_by: Address,
}

// --- Storage Keys ---

#[contracttype]
pub enum DataKey {
    Admin,
    PendingAdmin,
    NextMerchantId,
    Merchant(u64),
    MerchantName(String),
    FreedName(String),
    ArchivedMerchant(u64),
    VerifiedCount(u64),
    Verifiers,
    MerchantIds,
    CategoryIndex(Symbol),
    MetadataCooldown,
    MetadataCooldownConfig,
    MerchantVerifier(u64, Address),
    MerchantVerifierList(u64),
    VerificationPolicy(u64),
    LastMetadataUpdate(u64),
    GlobalReputationContract,
    Categories,
}

/// Mirror of `ReputationScore` from `delego-reputation` for cross-contract deserialization.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExternalReputationScore {
    pub entity: Address,
    pub score: u32,
    pub total_transactions: u64,
    pub successful_transactions: u64,
    pub disputed_transactions: u64,
    pub avg_rating: u32,
    pub last_updated: u64,
}

const MAX_COMMISSION_BPS: u32 = 10_000;
const MAX_REQUIRED_VERIFICATIONS: u32 = 50;
const DEFAULT_METADATA_COOLDOWN_SECS: u64 = 86_400; // 24 hours
const MIN_METADATA_COOLDOWN_SECS: u64 = 60;
const MAX_METADATA_COOLDOWN_SECS: u64 = 30 * 24 * 60 * 60;
const MAX_PAGE_LIMIT: u32 = 50;
const PERSISTENT_BUMP_THRESHOLD: u32 = 17_280; // ~1 day of ledgers (5s/ledger)
const PERSISTENT_BUMP_AMOUNT: u32 = 518_400; // ~30 days of ledgers

pub(crate) fn normalize_symbol(env: &Env, sym: &Symbol) -> Symbol {
    use soroban_sdk::xdr::ToXdr;
    let xdr = sym.clone().to_xdr(env);
    let xdr_len = xdr.len() as usize;
    // XDR encoding of ScVal::Symbol:
    // 4-byte tag (ScValType::Symbol = 10) + 4-byte big-endian length + characters + 0..3 padding bytes
    if xdr_len >= 8 {
        let mut xdr_bytes = [0u8; 48];
        let read_len = xdr_len.min(48);
        xdr.copy_into_slice(&mut xdr_bytes[..read_len]);
        let sym_len =
            u32::from_be_bytes([xdr_bytes[4], xdr_bytes[5], xdr_bytes[6], xdr_bytes[7]]) as usize;
        if sym_len <= 32 && 8 + sym_len <= read_len {
            let mut lower_buf = [0u8; 32];
            for i in 0..sym_len {
                lower_buf[i] = xdr_bytes[8 + i].to_ascii_lowercase();
            }
            if let Ok(lower_str) = core::str::from_utf8(&lower_buf[..sym_len]) {
                return Symbol::new(env, lower_str);
            }
        }
    }
    sym.clone()
}

#[contract]
pub struct MarketplaceContract;

#[contractimpl]
impl MarketplaceContract {
    // --- Initialization ---

    pub fn __constructor(env: Env, admin: Address) -> Result<(), MarketplaceError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(MarketplaceError::AlreadyInitialized);
        }
        admin.require_auth();

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &Option::<Address>::None);
        env.storage()
            .instance()
            .set(&DataKey::NextMerchantId, &1u64);
        env.storage()
            .instance()
            .set(&DataKey::Verifiers, &Vec::<Verifier>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::Categories, &Vec::<CategoryEntry>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::MetadataCooldown, &DEFAULT_METADATA_COOLDOWN_SECS);
        env.storage().instance().set(
            &DataKey::MetadataCooldownConfig,
            &CooldownConfig {
                value_seconds: DEFAULT_METADATA_COOLDOWN_SECS,
                min_seconds: MIN_METADATA_COOLDOWN_SECS,
                max_seconds: MAX_METADATA_COOLDOWN_SECS,
            },
        );

        Ok(())
    }

    // --- Merchant Lifecycle ---

    pub fn register_merchant(
        env: Env,
        merchant: Address,
        params: RegisterParams,
    ) -> Result<u64, MarketplaceError> {
        merchant.require_auth();

        if params.name.is_empty() {
            return Err(MarketplaceError::InvalidParam);
        }

        let name_key = DataKey::MerchantName(params.name.clone());
        if env.storage().persistent().has(&name_key) {
            return Err(MarketplaceError::DuplicateMerchantName);
        }

        // If the name was previously freed, clear the FreedName entry so it can be reused.
        let freed_key = DataKey::FreedName(params.name.clone());
        if env.storage().persistent().has(&freed_key) {
            env.storage().persistent().remove(&freed_key);
        }

        let normalized_category = normalize_symbol(&env, &params.category);

        // Allowlist gating: When non-empty, only allowlisted categories are accepted
        let categories = Self::get_categories(env.clone());
        if !categories.is_empty() {
            let mut allowed = false;
            for c in categories.iter() {
                if c.normalized == normalized_category {
                    allowed = true;
                    break;
                }
            }
            if !allowed {
                return Err(MarketplaceError::InvalidCategory);
            }
        }

        let now = env.ledger().timestamp();
        let next_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextMerchantId)
            .ok_or(MarketplaceError::NotInitialized)?;

        let required_verifications = if params.required_verifications == 0 {
            1
        } else {
            params.required_verifications
        };

        // Determine current verifier capacity and enforce bounds
        let verifiers_len: u32 = Self::get_verifiers(env.clone()).len();
        if required_verifications > MAX_REQUIRED_VERIFICATIONS {
            return Err(MarketplaceError::InvalidParam);
        }
        if verifiers_len > 0 && required_verifications > verifiers_len {
            return Err(MarketplaceError::InvalidParam);
        }

        let new_merchant = Merchant {
            id: next_id,
            owner: Some(merchant.clone()),
            name: params.name.clone(),
            description: params.description,
            category: normalized_category.clone(),
            image_url: params.image_url,
            commission_rate_bps: 0,
            metadata: params.metadata.clone(),
            status: MerchantStatus::Registered,
            verified: false,
            created_at: now,
            updated_at: now,
            reputation: None,
        };

        // Persist merchant record and indices
        env.storage()
            .persistent()
            .set(&DataKey::Merchant(next_id), &new_merchant);
        env.storage().persistent().set(&name_key, &next_id);
        let policy = VerificationPolicy {
            required: required_verifications,
            max_verifications: verifiers_len.min(MAX_REQUIRED_VERIFICATIONS),
        };
        env.storage()
            .persistent()
            .set(&DataKey::VerificationPolicy(next_id), &policy);
        env.storage()
            .persistent()
            .set(&DataKey::VerifiedCount(next_id), &0u32);
        env.storage().persistent().set(
            &DataKey::MerchantVerifierList(next_id),
            &Vec::<Address>::new(&env),
        );
        env.storage()
            .persistent()
            .set(&DataKey::LastMetadataUpdate(next_id), &now);

        // Append to full merchant ids index (persistent storage)
        let mut merchant_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantIds)
            .unwrap_or_else(|| Vec::new(&env));
        merchant_ids.push_back(next_id);
        env.storage()
            .persistent()
            .set(&DataKey::MerchantIds, &merchant_ids);

        // Append to category index
        let cat_key = DataKey::CategoryIndex(normalized_category.clone());
        let mut cat_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&cat_key)
            .unwrap_or_else(|| Vec::new(&env));
        cat_ids.push_back(next_id);
        env.storage().persistent().set(&cat_key, &cat_ids);

        // Extend TTL for all persistent entries created
        let storage = env.storage().persistent();
        storage.extend_ttl(
            &DataKey::Merchant(next_id),
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        storage.extend_ttl(&name_key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
        storage.extend_ttl(
            &DataKey::VerificationPolicy(next_id),
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        storage.extend_ttl(
            &DataKey::VerifiedCount(next_id),
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        storage.extend_ttl(
            &DataKey::MerchantVerifierList(next_id),
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        storage.extend_ttl(
            &DataKey::LastMetadataUpdate(next_id),
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        storage.extend_ttl(
            &DataKey::MerchantIds,
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        storage.extend_ttl(&cat_key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT);

        // Increment monotonic counter
        let incremented = next_id
            .checked_add(1)
            .ok_or(MarketplaceError::InvalidParam)?;
        env.storage()
            .instance()
            .set(&DataKey::NextMerchantId, &incremented);

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("reg")),
            MerchantRegisteredEvent {
                merchant_id: next_id,
                owner: merchant,
                name: params.name,
            },
        );

        Ok(next_id)
    }

    pub fn is_name_available(env: Env, name: String) -> bool {
        let live_key = DataKey::MerchantName(name);
        !env.storage().persistent().has(&live_key)
    }

    pub fn update_merchant_profile(
        env: Env,
        merchant_id: u64,
        caller: Address,
        name: String,
        description: String,
        image_url: String,
    ) -> Result<(), MarketplaceError> {
        caller.require_auth();

        if name.is_empty() {
            return Err(MarketplaceError::InvalidParam);
        }

        let mut merchant = Self::get_merchant(env.clone(), merchant_id)?;
        Self::check_not_frozen_or_closed(&merchant)?;

        if merchant.owner != Some(caller) {
            return Err(MarketplaceError::Unauthorized);
        }

        if merchant.name != name {
            let new_name_key = DataKey::MerchantName(name.clone());
            if env.storage().persistent().has(&new_name_key) {
                return Err(MarketplaceError::DuplicateMerchantName);
            }
            env.storage()
                .persistent()
                .remove(&DataKey::MerchantName(merchant.name.clone()));
            env.storage().persistent().set(&new_name_key, &merchant_id);
            merchant.name = name;
        }

        merchant.description = description;
        merchant.image_url = image_url;
        merchant.updated_at = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::Merchant(merchant_id), &merchant);

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("profile")),
            MerchantProfileUpdatedEvent {
                merchant_id,
                updated_fields: symbol_short!("profile"),
            },
        );

        Ok(())
    }

    pub fn update_metadata(
        env: Env,
        merchant_id: u64,
        caller: Address,
        new_metadata: Option<String>,
    ) -> Result<(), MarketplaceError> {
        caller.require_auth();

        let mut merchant = Self::get_merchant(env.clone(), merchant_id)?;
        Self::check_not_frozen_or_closed(&merchant)?;

        let admin = Self::get_admin(env.clone())?;
        let is_admin = caller == admin;
        let is_owner = merchant.owner == Some(caller.clone());

        if !is_admin && !is_owner {
            return Err(MarketplaceError::Unauthorized);
        }

        // Change-detection: Compare incoming metadata against stored value
        if merchant.metadata == new_metadata {
            // No change detected; skip write and event emission
            return Ok(());
        }

        let now = env.ledger().timestamp();

        if !is_admin {
            let cooldown = Self::get_metadata_cooldown(env.clone());
            let last_update: u64 = env
                .storage()
                .persistent()
                .get(&DataKey::LastMetadataUpdate(merchant_id))
                .unwrap_or(0);
            if cooldown > 0 && now < last_update.saturating_add(cooldown) {
                return Err(MarketplaceError::MetadataLockActive);
            }
        }

        merchant.metadata = new_metadata.clone();
        merchant.updated_at = now;

        env.storage()
            .persistent()
            .set(&DataKey::Merchant(merchant_id), &merchant);
        env.storage()
            .persistent()
            .set(&DataKey::LastMetadataUpdate(merchant_id), &now);

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("meta")),
            MerchantMetadataUpdatedEvent {
                merchant_id,
                new_metadata,
            },
        );

        Ok(())
    }

    // --- Category Management ---

    pub fn add_category(
        env: Env,
        admin: Address,
        category: CategoryEntry,
    ) -> Result<(), MarketplaceError> {
        admin.require_auth();
        let current_admin = Self::get_admin(env.clone())?;
        if admin != current_admin {
            return Err(MarketplaceError::Unauthorized);
        }

        let normalized = normalize_symbol(&env, &category.key);
        let mut categories = Self::get_categories(env.clone());
        for c in categories.iter() {
            if c.normalized == normalized || c.key == category.key {
                return Err(MarketplaceError::InvalidCategory);
            }
        }

        let now = env.ledger().timestamp();
        let entry = CategoryEntry {
            key: category.key.clone(),
            normalized: normalized.clone(),
            display: category.display,
            added_at: if category.added_at == 0 {
                now
            } else {
                category.added_at
            },
        };

        categories.push_back(entry);
        env.storage()
            .instance()
            .set(&DataKey::Categories, &categories);

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("cat_add")),
            CategoryAddedEvent {
                key: category.key,
                normalized,
                added_by: admin,
            },
        );

        Ok(())
    }

    pub fn remove_category(
        env: Env,
        admin: Address,
        category: Symbol,
    ) -> Result<(), MarketplaceError> {
        admin.require_auth();
        let current_admin = Self::get_admin(env.clone())?;
        if admin != current_admin {
            return Err(MarketplaceError::Unauthorized);
        }

        let normalized = normalize_symbol(&env, &category);
        let categories = Self::get_categories(env.clone());
        let mut new_categories = Vec::new(&env);
        let mut found = false;

        for c in categories.iter() {
            if c.key == category || c.normalized == normalized {
                found = true;
            } else {
                new_categories.push_back(c);
            }
        }

        if !found {
            return Err(MarketplaceError::InvalidCategory);
        }

        env.storage()
            .instance()
            .set(&DataKey::Categories, &new_categories);

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("cat_rem")),
            CategoryRemovedEvent {
                key: category,
                removed_by: admin,
            },
        );

        Ok(())
    }

    pub fn get_categories(env: Env) -> Vec<CategoryEntry> {
        env.storage()
            .instance()
            .get(&DataKey::Categories)
            .unwrap_or_else(|| Vec::new(&env))
    }

    // --- Verification ---

    pub fn add_verifier(
        env: Env,
        admin: Address,
        verifier: Verifier,
    ) -> Result<(), MarketplaceError> {
        admin.require_auth();
        let current_admin = Self::get_admin(env.clone())?;
        if admin != current_admin {
            return Err(MarketplaceError::Unauthorized);
        }

        let mut verifiers = Self::get_verifiers(env.clone());
        for v in verifiers.iter() {
            if v.address == verifier.address {
                return Err(MarketplaceError::VerifierAlreadyExists);
            }
        }

        verifiers.push_back(verifier.clone());
        env.storage()
            .instance()
            .set(&DataKey::Verifiers, &verifiers);

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("v_add")),
            VerifierAddedEvent {
                verifier: verifier.address,
                label: verifier.label,
                added_by: admin,
            },
        );

        Ok(())
    }

    pub fn remove_verifier(
        env: Env,
        admin: Address,
        verifier: Address,
    ) -> Result<(), MarketplaceError> {
        admin.require_auth();
        let current_admin = Self::get_admin(env.clone())?;
        if admin != current_admin {
            return Err(MarketplaceError::Unauthorized);
        }

        let verifiers = Self::get_verifiers(env.clone());
        let mut new_verifiers = Vec::new(&env);
        let mut found = false;

        for v in verifiers.iter() {
            if v.address == verifier {
                found = true;
            } else {
                new_verifiers.push_back(v);
            }
        }

        if !found {
            return Err(MarketplaceError::VerifierNotFound);
        }

        // Ensure removal won't invalidate any existing merchant verification policies
        let new_verifiers_len: u32 = new_verifiers.len();
        let merchant_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantIds)
            .unwrap_or_else(|| Vec::new(&env));
        for id in merchant_ids.iter() {
            let policy: Option<VerificationPolicy> = env
                .storage()
                .persistent()
                .get(&DataKey::VerificationPolicy(id));
            if let Some(p) = policy {
                if p.required > new_verifiers_len {
                    return Err(MarketplaceError::InvalidParam);
                }
            }
        }

        env.storage()
            .instance()
            .set(&DataKey::Verifiers, &new_verifiers);

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("v_rem")),
            VerifierRemovedEvent {
                verifier,
                removed_by: admin,
            },
        );

        Ok(())
    }

    pub fn verify_merchant(
        env: Env,
        merchant_id: u64,
        verifier: Address,
    ) -> Result<(), MarketplaceError> {
        verifier.require_auth();

        let verifiers = Self::get_verifiers(env.clone());
        let mut is_registered_verifier = false;
        for v in verifiers.iter() {
            if v.address == verifier {
                is_registered_verifier = true;
                break;
            }
        }

        if !is_registered_verifier {
            return Err(MarketplaceError::Unauthorized);
        }

        let mut merchant = Self::get_merchant(env.clone(), merchant_id)?;
        Self::check_not_frozen_or_closed(&merchant)?;

        if merchant.verified {
            return Err(MarketplaceError::AlreadyVerified);
        }

        let verifier_key = DataKey::MerchantVerifier(merchant_id, verifier.clone());
        if env.storage().persistent().has(&verifier_key) {
            return Err(MarketplaceError::AlreadyVerified);
        }

        env.storage().persistent().set(&verifier_key, &true);

        let mut verifier_list: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantVerifierList(merchant_id))
            .unwrap_or_else(|| Vec::new(&env));
        verifier_list.push_back(verifier.clone());
        env.storage()
            .persistent()
            .set(&DataKey::MerchantVerifierList(merchant_id), &verifier_list);

        let current_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::VerifiedCount(merchant_id))
            .unwrap_or(0);
        let new_count = current_count
            .checked_add(1)
            .ok_or(MarketplaceError::VerificationCountOverflow)?;
        env.storage()
            .persistent()
            .set(&DataKey::VerifiedCount(merchant_id), &new_count);

        let required: u32 = {
            let policy: Option<VerificationPolicy> = env
                .storage()
                .persistent()
                .get(&DataKey::VerificationPolicy(merchant_id));
            policy.map(|p| p.required).unwrap_or(1)
        };

        if new_count >= required {
            merchant.verified = true;
            merchant.status = MerchantStatus::Verified;
        }
        merchant.updated_at = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::Merchant(merchant_id), &merchant);

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("verif")),
            MerchantVerifiedEvent {
                merchant_id,
                verifier,
                verified: merchant.verified,
            },
        );

        Ok(())
    }

    pub fn revoke_verification(
        env: Env,
        admin: Address,
        merchant_id: u64,
    ) -> Result<(), MarketplaceError> {
        admin.require_auth();
        let current_admin = Self::get_admin(env.clone())?;
        if admin != current_admin {
            return Err(MarketplaceError::Unauthorized);
        }

        let mut merchant = Self::get_merchant(env.clone(), merchant_id)?;
        Self::check_not_frozen_or_closed(&merchant)?;

        merchant.verified = false;
        if merchant.status == MerchantStatus::Verified {
            merchant.status = MerchantStatus::Registered;
        }
        merchant.updated_at = env.ledger().timestamp();

        // Clear collected verifications
        env.storage()
            .persistent()
            .set(&DataKey::VerifiedCount(merchant_id), &0u32);

        let verifier_list: Vec<Address> = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantVerifierList(merchant_id))
            .unwrap_or_else(|| Vec::new(&env));
        for v in verifier_list.iter() {
            env.storage()
                .persistent()
                .remove(&DataKey::MerchantVerifier(merchant_id, v));
        }
        env.storage().persistent().set(
            &DataKey::MerchantVerifierList(merchant_id),
            &Vec::<Address>::new(&env),
        );

        env.storage()
            .persistent()
            .set(&DataKey::Merchant(merchant_id), &merchant);

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("v_rev")),
            MerchantVerificationRevokedEvent {
                merchant_id,
                revoked_by: admin,
            },
        );

        Ok(())
    }

    // --- Discovery & Query ---

    pub fn get_merchant(env: Env, merchant_id: u64) -> Result<Merchant, MarketplaceError> {
        let merchant: Merchant = env
            .storage()
            .persistent()
            .get(&DataKey::Merchant(merchant_id))
            .ok_or(MarketplaceError::MerchantNotFound)?;

        let storage = env.storage().persistent();
        storage.extend_ttl(
            &DataKey::Merchant(merchant_id),
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
        storage.extend_ttl(
            &DataKey::MerchantName(merchant.name.clone()),
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );

        Ok(merchant)
    }

    pub fn get_merchant_view_detailed(env: Env, merchant_id: u64) -> Result<MerchantViewDetailed, MarketplaceError> {
        let merchant = Self::get_merchant(env.clone(), merchant_id)?;

        let reputation_contract = merchant.reputation.clone().or_else(|| {
            env.storage()
                .instance()
                .get(&DataKey::GlobalReputationContract)
        });

        let reputation = if let Some(rep_addr) = reputation_contract {
            if let Some(owner) = merchant.owner.clone() {
                let args = soroban_sdk::vec![&env, owner.to_val()];
                let call_result = env.try_invoke_contract::<ExternalReputationScore, InvokeError>(
                    &rep_addr,
                    &Symbol::new(&env, "get_reputation"),
                    args,
                );
                match call_result {
                    Ok(Ok(rep)) => ReputationResolution::Available(rep.score, rep.last_updated),
                    _ => ReputationResolution::CallFailed,
                }
            } else {
                ReputationResolution::NotConfigured
            }
        } else {
            ReputationResolution::NotConfigured
        };

        let reputation_score = match &reputation {
            ReputationResolution::Available(score, _) => Some(*score),
            _ => None,
        };

        let view = MerchantView {
            id: merchant.id,
            name: merchant.name,
            category: merchant.category,
            commission_rate_bps: merchant.commission_rate_bps,
            verified: merchant.verified,
            status: merchant.status,
            reputation_score,
        };

        Ok(MerchantViewDetailed {
            view,
            reputation,
        })
    }

    pub fn get_merchant_view(env: Env, merchant_id: u64) -> Result<MerchantView, MarketplaceError> {
        let detailed = Self::get_merchant_view_detailed(env, merchant_id)?;
        Ok(detailed.view)
    }

    pub fn get_merchants(
        env: Env,
        offset: u32,
        limit: u32,
    ) -> Result<DiscoveryPage, MarketplaceError> {
        // Return all merchants regardless of status (backward compatible)
        Self::get_merchants_filtered(env, offset, limit, None)
    }

    pub fn get_merchants_by_status(
        env: Env,
        status: MerchantStatus,
        offset: u32,
        limit: u32,
    ) -> Result<DiscoveryPage, MarketplaceError> {
        // Filter merchants by specific status
        Self::get_merchants_filtered(env, offset, limit, Some(status))
    }

    fn get_merchants_filtered(
        env: Env,
        offset: u32,
        limit: u32,
        status_filter: Option<MerchantStatus>,
    ) -> Result<DiscoveryPage, MarketplaceError> {
        let limit = limit.min(MAX_PAGE_LIMIT);

        // Probe id-scoped `DataKey::Merchant(u64)` entries directly instead of
        // deserializing the whole `MerchantIds` index (issue #120). Merchant ids
        // are assigned monotonically from `NextMerchantId`, so the id space is
        // contiguous and each id in `1..NextMerchantId` maps to a record.
        let next_merchant_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextMerchantId)
            .unwrap_or(1);

        // Filter merchants by status if provided
        let mut filtered_ids = Vec::new(&env);
        let mut id = 1u64;
        while id < next_merchant_id {
            if let Ok(merchant) = Self::get_merchant(env.clone(), id) {
                if let Some(status) = status_filter {
                    if merchant.status == status {
                        filtered_ids.push_back(id);
                    }
                } else {
                    // No filter: include all
                    filtered_ids.push_back(id);
                }
            }
            id += 1;
        }

        let total = filtered_ids.len();
        if offset >= total || limit == 0 {
            return Ok(DiscoveryPage {
                items: Vec::new(&env),
                total,
                next_offset: None,
                next_cursor: None,
            });
        }

        let end = offset.saturating_add(limit).min(total);
        let mut items = Vec::new(&env);
        let mut i = offset;
        while i < end {
            let id = filtered_ids.get(i).unwrap();
            let view = Self::get_merchant_view(env.clone(), id)?;
            items.push_back(view);
            i += 1;
        }

        let next_offset = if end < total { Some(end) } else { None };
        // Expose the last returned id so callers can pivot to cursor pagination.
        let next_cursor = if end < total {
            Some(filtered_ids.get(end.saturating_sub(1)).unwrap())
        } else {
            None
        };

        Ok(DiscoveryPage {
            items,
            total,
            next_offset,
            next_cursor,
        })
    }

    pub fn get_merchants_by_category(
        env: Env,
        category: Symbol,
        offset: u32,
        limit: u32,
    ) -> Result<DiscoveryPage, MarketplaceError> {
        // Return all merchants in category regardless of status (backward compatible)
        Self::get_merchants_by_category_filtered(env, category, offset, limit, None)
    }

    pub fn get_merchants_by_category_status(
        env: Env,
        category: Symbol,
        status: MerchantStatus,
        offset: u32,
        limit: u32,
    ) -> Result<DiscoveryPage, MarketplaceError> {
        // Filter merchants by category and specific status
        Self::get_merchants_by_category_filtered(env, category, offset, limit, Some(status))
    }

    fn get_merchants_by_category_filtered(
        env: Env,
        category: Symbol,
        offset: u32,
        limit: u32,
        status_filter: Option<MerchantStatus>,
    ) -> Result<DiscoveryPage, MarketplaceError> {
        let limit = limit.min(MAX_PAGE_LIMIT);
        let normalized = normalize_symbol(&env, &category);
        let mut cat_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::CategoryIndex(normalized.clone()))
            .unwrap_or_else(|| Vec::new(&env));

        // Backward compatibility: If no merchants found under normalized key,
        // and raw category differs from normalized, check raw category index.
        if cat_ids.is_empty() && normalized != category {
            if let Some(legacy_ids) = env
                .storage()
                .persistent()
                .get(&DataKey::CategoryIndex(category))
            {
                cat_ids = legacy_ids;
            }
        }

        // Filter merchants by status if provided
        let mut filtered_ids = Vec::new(&env);
        for id in cat_ids.iter() {
            if let Ok(merchant) = Self::get_merchant(env.clone(), id) {
                if let Some(status) = status_filter {
                    if merchant.status == status {
                        filtered_ids.push_back(id);
                    }
                } else {
                    // No filter: include all
                    filtered_ids.push_back(id);
                }
            }
        }

        let total = filtered_ids.len();
        if offset >= total || limit == 0 {
            return Ok(DiscoveryPage {
                items: Vec::new(&env),
                total,
                next_offset: None,
                next_cursor: None,
            });
        }

        let end = offset.saturating_add(limit).min(total);
        let mut items = Vec::new(&env);
        let mut i = offset;
        while i < end {
            let id = filtered_ids.get(i).unwrap();
            let view = Self::get_merchant_view(env.clone(), id)?;
            items.push_back(view);
            i += 1;
        }

        let next_offset = if end < total { Some(end) } else { None };
        // Expose the last returned id so callers can pivot to cursor pagination.
        let next_cursor = if end < total {
            Some(filtered_ids.get(end.saturating_sub(1)).unwrap())
        } else {
            None
        };

        Ok(DiscoveryPage {
            items,
            total,
            next_offset,
            next_cursor,
        })
    }

    pub fn get_merchants_cursor(
        env: Env,
        cursor: MerchantCursor,
    ) -> Result<DiscoveryPage, MarketplaceError> {
        let limit = cursor.limit.min(MAX_PAGE_LIMIT);
        let status_filter = cursor.status;

        // Iterate id-scoped `DataKey::Merchant(u64)` entries from `after_id`
        // without deserializing the whole `MerchantIds` index (issue #120). The
        // loop reads at most `limit` merchant views, so cost is bounded by the
        // page size (plus cheap key probes for skipped ids), not by the registry
        // size, and deep pages never pay for the full index.
        let next_merchant_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextMerchantId)
            .unwrap_or(1);

        let mut items = Vec::new(&env);
        let mut last_id = cursor.after_id;
        let mut id = cursor.after_id.saturating_add(1);

        while items.len() < limit && id < next_merchant_id {
            if env.storage().persistent().has(&DataKey::Merchant(id)) {
                let merchant = Self::get_merchant(env.clone(), id)?;
                let matches =
                    status_filter == MerchantStatus::All || merchant.status == status_filter;
                if matches {
                    let view = Self::get_merchant_view(env.clone(), id)?;
                    items.push_back(view);
                    last_id = id;
                }
            }
            id += 1;
        }

        let next_cursor = if !items.is_empty() && id < next_merchant_id {
            Some(last_id)
        } else {
            None
        };

        Ok(DiscoveryPage {
            items,
            total: 0,
            next_offset: None,
            next_cursor,
        })
    }

    pub fn get_merchants_by_category_cursor(
        env: Env,
        category: Symbol,
        after_id: u64,
        limit: u32,
    ) -> Result<DiscoveryPage, MarketplaceError> {
        let limit = limit.min(MAX_PAGE_LIMIT);

        // Category membership is already scoped by `CategoryIndex(category)`, so
        // page that id list from `after_id` without scanning the global registry.
        let cat_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::CategoryIndex(category))
            .unwrap_or_else(|| Vec::new(&env));

        let mut items = Vec::new(&env);
        let mut last_id = after_id;
        let n = cat_ids.len();

        // Skip ids already seen by the caller (after_id is exclusive).
        let mut i = 0u32;
        while i < n && cat_ids.get(i).unwrap() <= after_id {
            i += 1;
        }

        while i < n && items.len() < limit {
            let id = cat_ids.get(i).unwrap();
            let view = Self::get_merchant_view(env.clone(), id)?;
            items.push_back(view);
            last_id = id;
            i += 1;
        }

        let next_cursor = if i < n { Some(last_id) } else { None };

        Ok(DiscoveryPage {
            items,
            total: 0,
            next_offset: None,
            next_cursor,
        })
    }

    // --- Commission ---

    pub fn set_merchant_commission(
        env: Env,
        merchant_id: u64,
        caller: Address,
        commission_bps: u32,
    ) -> Result<(), MarketplaceError> {
        caller.require_auth();

        if commission_bps > MAX_COMMISSION_BPS {
            return Err(MarketplaceError::InvalidCommissionBps);
        }

        let mut merchant = Self::get_merchant(env.clone(), merchant_id)?;
        Self::check_not_frozen_or_closed(&merchant)?;

        let admin = Self::get_admin(env.clone())?;
        let is_admin = caller == admin;
        let is_owner = merchant.owner == Some(caller.clone());

        if !is_admin && !is_owner {
            return Err(MarketplaceError::Unauthorized);
        }

        merchant.commission_rate_bps = commission_bps;
        merchant.updated_at = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::Merchant(merchant_id), &merchant);

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("comm")),
            MerchantCommissionSetEvent {
                merchant_id,
                commission_bps,
                set_by: caller,
            },
        );

        Ok(())
    }

    pub fn get_commission(env: Env, merchant_id: u64) -> Result<u32, MarketplaceError> {
        let merchant = Self::get_merchant(env, merchant_id)?;
        Ok(merchant.commission_rate_bps)
    }

    // --- Moderation / Security ---

    pub fn suspend_merchant(
        env: Env,
        admin: Address,
        merchant_id: u64,
    ) -> Result<(), MarketplaceError> {
        admin.require_auth();
        let current_admin = Self::get_admin(env.clone())?;
        if admin != current_admin {
            return Err(MarketplaceError::Unauthorized);
        }

        let mut merchant = Self::get_merchant(env.clone(), merchant_id)?;
        if matches!(merchant.status, MerchantStatus::Closed) {
            return Err(MarketplaceError::MerchantClosed);
        }

        merchant.status = MerchantStatus::Suspended;
        merchant.updated_at = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::Merchant(merchant_id), &merchant);

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("suspend")),
            MerchantSuspendedEvent {
                merchant_id,
                suspended_by: admin,
            },
        );

        Ok(())
    }

    pub fn unsuspend_merchant(
        env: Env,
        admin: Address,
        merchant_id: u64,
    ) -> Result<(), MarketplaceError> {
        admin.require_auth();
        let current_admin = Self::get_admin(env.clone())?;
        if admin != current_admin {
            return Err(MarketplaceError::Unauthorized);
        }

        let mut merchant = Self::get_merchant(env.clone(), merchant_id)?;
        if matches!(merchant.status, MerchantStatus::Closed) {
            return Err(MarketplaceError::MerchantClosed);
        }

        merchant.status = if merchant.verified {
            MerchantStatus::Verified
        } else {
            MerchantStatus::Registered
        };
        merchant.updated_at = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::Merchant(merchant_id), &merchant);

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("unsusp")),
            MerchantUnsuspendedEvent {
                merchant_id,
                unsuspended_by: admin,
            },
        );

        Ok(())
    }

    pub fn close_merchant(
        env: Env,
        admin: Address,
        merchant_id: u64,
        reason: Symbol,
    ) -> Result<(), MarketplaceError> {
        admin.require_auth();
        let current_admin = Self::get_admin(env.clone())?;
        if admin != current_admin {
            return Err(MarketplaceError::Unauthorized);
        }

        let mut merchant = Self::get_merchant(env.clone(), merchant_id)?;
        merchant.status = MerchantStatus::Closed;
        merchant.updated_at = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::Merchant(merchant_id), &merchant);

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("closed")),
            MerchantClosedEvent {
                merchant_id,
                closed_by: admin,
                reason,
                name: merchant.name.clone(),
                category: merchant.category,
            },
        );

        Ok(())
    }

    /// Prunes closed merchants from `MerchantIds` and `CategoryIndex` (state maintenance).
    ///
    /// Callable by admin in bounded batches (`merchant_ids.len() <= MAX_PAGE_LIMIT`).
    /// Returns the number of closed merchants pruned from active indices.
    pub fn prune_closed_merchants(
        env: Env,
        admin: Address,
        merchant_ids: Vec<u64>,
    ) -> Result<u32, MarketplaceError> {
        admin.require_auth();
        let current_admin = Self::get_admin(env.clone())?;
        if admin != current_admin {
            return Err(MarketplaceError::Unauthorized);
        }
        if merchant_ids.len() > MAX_PAGE_LIMIT {
            return Err(MarketplaceError::InvalidParam);
        }

        let mut all_ids: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::MerchantIds)
            .unwrap_or_else(|| Vec::new(&env));
        let mut pruned_count: u32 = 0;

        for id in merchant_ids.iter() {
            if let Ok(merchant) = Self::get_merchant(env.clone(), id) {
                if merchant.status == MerchantStatus::Closed {
                    let mut modified = false;
                    if let Some(pos) = all_ids.iter().position(|x| x == id) {
                        all_ids.remove(pos as u32);
                        modified = true;
                    }
                    let cat_key = DataKey::CategoryIndex(merchant.category.clone());
                    if let Some(mut cat_ids) =
                        env.storage().persistent().get::<_, Vec<u64>>(&cat_key)
                    {
                        if let Some(pos) = cat_ids.iter().position(|x| x == id) {
                            cat_ids.remove(pos as u32);
                            env.storage().persistent().set(&cat_key, &cat_ids);
                        }
                    }
                    if modified {
                        pruned_count += 1;
                    }
                }
            }
        }

        if pruned_count > 0 {
            env.storage()
                .persistent()
                .set(&DataKey::MerchantIds, &all_ids);
            env.events().publish(
                (symbol_short!("mkplc"), symbol_short!("pruned")),
                MerchantPrunedEvent {
                    pruned_count,
                    pruned_by: admin,
                },
            );
        }

        Ok(pruned_count)
    }

    // --- Reputation Pairing Config ---

    pub fn set_merchant_reputation(
        env: Env,
        admin: Address,
        merchant_id: u64,
        reputation: Option<Address>,
    ) -> Result<(), MarketplaceError> {
        admin.require_auth();
        let current_admin = Self::get_admin(env.clone())?;
        if admin != current_admin {
            return Err(MarketplaceError::Unauthorized);
        }

        let mut merchant = Self::get_merchant(env.clone(), merchant_id)?;
        Self::check_not_frozen_or_closed(&merchant)?;

        merchant.reputation = reputation.clone();
        merchant.updated_at = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::Merchant(merchant_id), &merchant);

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("rep_set")),
            MerchantReputationSetEvent {
                merchant_id,
                reputation,
                set_by: admin,
            },
        );

        Ok(())
    }

    pub fn set_reputation_contract(
        env: Env,
        admin: Address,
        reputation: Address,
    ) -> Result<(), MarketplaceError> {
        admin.require_auth();
        let current_admin = Self::get_admin(env.clone())?;
        if admin != current_admin {
            return Err(MarketplaceError::Unauthorized);
        }

        env.storage()
            .instance()
            .set(&DataKey::GlobalReputationContract, &reputation);

        Ok(())
    }

    // --- Admin / Config ---

    pub fn propose_admin(
        env: Env,
        current_admin: Address,
        new_admin: Address,
    ) -> Result<bool, MarketplaceError> {
        current_admin.require_auth();
        let admin = Self::get_admin(env.clone())?;
        if current_admin != admin {
            return Err(MarketplaceError::Unauthorized);
        }

        if new_admin == current_admin {
            return Ok(false);
        }

        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &Some(new_admin.clone()));

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("adm_prop")),
            AdminProposedEvent {
                current_admin,
                new_admin,
            },
        );

        Ok(true)
    }

    pub fn accept_admin(env: Env, caller: Address) -> Result<(), MarketplaceError> {
        caller.require_auth();

        let pending: Option<Address> = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .unwrap_or(None);

        // No proposal exists: distinct error so callers can tell this apart
        // from "not the proposed successor" (Unauthorized).
        if pending.is_none() {
            return Err(MarketplaceError::NoPendingAdmin);
        }

        if pending != Some(caller.clone()) {
            return Err(MarketplaceError::Unauthorized);
        }

        let previous_admin = Self::get_admin(env.clone())?;

        env.storage().instance().set(&DataKey::Admin, &caller);
        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &Option::<Address>::None);

        env.events().publish(
            (symbol_short!("mkplc"), symbol_short!("adm_acc")),
            AdminAcceptedEvent {
                previous_admin,
                new_admin: caller,
            },
        );

        Ok(())
    }

    pub fn set_metadata_cooldown(
        env: Env,
        admin: Address,
        cooldown_seconds: u64,
    ) -> Result<(), MarketplaceError> {
        admin.require_auth();
        let current_admin = Self::get_admin(env.clone())?;
        if admin != current_admin {
            return Err(MarketplaceError::Unauthorized);
        }

        let previous = Self::get_metadata_cooldown(env.clone());
        let current =
            cooldown_seconds.clamp(MIN_METADATA_COOLDOWN_SECS, MAX_METADATA_COOLDOWN_SECS);
        if previous == current {
            return Ok(());
        }

        env.storage().instance().set(
            &DataKey::MetadataCooldownConfig,
            &CooldownConfig {
                value_seconds: current,
                min_seconds: MIN_METADATA_COOLDOWN_SECS,
                max_seconds: MAX_METADATA_COOLDOWN_SECS,
            },
        );
        // Keep the original key populated for deployments upgraded from the
        // pre-config format and older readers.
        env.storage()
            .instance()
            .set(&DataKey::MetadataCooldown, &current);
        env.events().publish(
            (symbol_short!("mkplc"), Symbol::new(&env, "cooldown_set")),
            MetadataCooldownSetEvent {
                previous: Some(previous),
                current,
                set_by: admin,
            },
        );

        Ok(())
    }

    pub fn get_metadata_cooldown(env: Env) -> u64 {
        env.storage()
            .instance()
            .get::<_, CooldownConfig>(&DataKey::MetadataCooldownConfig)
            .map(|config| config.value_seconds)
            .or_else(|| env.storage().instance().get(&DataKey::MetadataCooldown))
            .unwrap_or(DEFAULT_METADATA_COOLDOWN_SECS)
    }

    pub fn get_admin(env: Env) -> Result<Address, MarketplaceError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(MarketplaceError::NotInitialized)
    }

    pub fn get_verifiers(env: Env) -> Vec<Verifier> {
        env.storage()
            .instance()
            .get(&DataKey::Verifiers)
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn version(_env: Env) -> ContractVersion {
        ContractVersion {
            name: symbol_short!("market"),
            semver: symbol_short!("0_2_0"),
            semver: soroban_sdk::Symbol::new(&_env, env!("CARGO_PKG_VERSION_SYM")),
        }
    }

    // --- Helper validation methods ---

    fn check_not_frozen_or_closed(merchant: &Merchant) -> Result<(), MarketplaceError> {
        match merchant.status {
            MerchantStatus::Suspended => Err(MarketplaceError::MerchantFrozen),
            MerchantStatus::Closed => Err(MarketplaceError::MerchantClosed),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod overflow_tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;

    #[test]
    fn verify_merchant_count_overflow_returns_error() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let contract_id = env.register_contract(None, MarketplaceContract);
        let client = MarketplaceContractClient::new(&env, &contract_id);

        client.__constructor(&admin).unwrap();

        let owner = Address::generate(&env);
        let params = RegisterParams {
            name: String::from_str(&env, "overflow-merchant"),
            description: String::from_str(&env, "desc"),
            category: symbol_short!("cate"),
            image_url: String::from_str(&env, "https://example.com/image.png"),
            metadata: None,
            required_verifications: 1,
        };

        let merchant_id = client.register_merchant(&owner, ¶ms).unwrap();

        env.as_contract(&contract_id, || {
            env.storage()
                .persistent()
                .set(&DataKey::VerifiedCount(merchant_id), &u32::MAX);
        });

        let verifier = Address::generate(&env);
        let verifier_struct = Verifier {
            address: verifier.clone(),
            label: symbol_short!("v"),
            registered_at: env.ledger().timestamp(),
        };
        client.add_verifier(&admin, &verifier_struct).unwrap();

        assert_eq!(
            client.verify_merchant(&merchant_id, &verifier),
            Err(MarketplaceError::VerificationCountOverflow)
        );
    }

    #[test]
    fn verification_count_overflow_error_payload() {
        assert_eq!(MarketplaceError::VerificationCountOverflow as u32, 16);
    }
}

#[cfg(test)]
mod test;
