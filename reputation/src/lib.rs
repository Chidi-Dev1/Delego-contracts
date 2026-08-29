//! Delego Reputation Contract
//!
//! Tracks time-decayed trust scores for merchants and agents on the Delego
//! platform, driven by escrow transaction outcomes and counterparty ratings.

#![no_std]
#![warn(missing_docs)]
// Several entry points mirror escrow/permissions call shapes and exceed
// clippy's default 7-argument limit; restructuring them would break the
// published ABI these contracts are reviewed against.
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, Env, String,
    Symbol, Vec,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationScore {
    pub entity: Address,
    /// 0-10000 basis points (0.00% to 100.00%). Masked to `0` by
    /// [`ReputationContract::get_reputation`] until `total_transactions`
    /// reaches `ReputationConfig::min_transactions_threshold`.
    pub score: u32,
    pub total_transactions: u64,
    pub successful_transactions: u64,
    pub disputed_transactions: u64,
    /// 0-10000 basis points of a 5-star scale, time-decayed like `score`.
    pub avg_rating: u32,
    pub last_updated: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransactionRecord {
    pub escrow_id: u64,
    pub entity: Address,
    pub counterparty: Address,
    pub amount: i128,
    pub outcome: TransactionOutcome,
    /// 0-10000, set once by `rate_entity`.
    pub rating: Option<u32>,
    pub recorded_at: u64,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TransactionOutcome {
    Released,
    Refunded,
    Disputed,
    ResolvedSeller,
    ResolvedBuyer,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Flag {
    pub reporter: Address,
    pub entity: Address,
    pub reason: Symbol,
    pub details: Option<String>,
    pub flagged_at: u64,
    pub resolved: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReputationConfig {
    pub decay_window_seconds: u64,
    pub min_transactions_threshold: u64,
    pub dispute_penalty_bps: u32,
    pub freeze_threshold_flags: u32,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum ReputationError {
    /// Reserved for API/ABI compatibility with issue #18's error contract.
    /// Unreachable in normal operation: initialization now happens via
    /// `__constructor` (see [`ReputationContract::__constructor`]), which
    /// the host guarantees can run at most once, atomically with
    /// deployment — there is no second call for this to guard against.
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    EntityNotFound = 4,
    /// Same escrow_id already rated.
    DuplicateRating = 5,
    /// Rating out of range.
    InvalidRating = 6,
    EntityFrozen = 7,
    /// Same reporter already flagged.
    AlreadyFlagged = 8,
    InvalidParam = 9,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct TransactionRecordedEvent {
    pub escrow_id: u64,
    pub entity: Address,
    pub outcome: TransactionOutcome,
    pub new_score: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct EntityRatedEvent {
    pub rater: Address,
    pub entity: Address,
    pub rating: u32,
    pub escrow_id: u64,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct EntityFlaggedEvent {
    pub reporter: Address,
    pub entity: Address,
    pub reason: Symbol,
    pub flag_count: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct EntityFrozenEvent {
    pub entity: Address,
    pub frozen_by: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct EntityUnfrozenEvent {
    pub entity: Address,
    pub unfrozen_by: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminProposedEvent {
    pub current_admin: Address,
    pub new_admin: Address,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminAcceptedEvent {
    pub new_admin: Address,
}

#[contracttype]
pub enum DataKey {
    Admin,
    PendingAdmin,
    Config,
    Reputation(Address),
    TransactionHistory(Address),
    TransactionRecord(u64),
    Flags(Address),
    FrozenStatus(Address),
    RatedEscrows(Address),
    /// `true` once `.1` has appeared as a `counterparty` on one of `.0`'s
    /// recorded transactions. Backs an O(1) [`ReputationContract::has_transacted_with`]
    /// check instead of scanning `TransactionHistory`.
    Transacted(Address, Address),
}

/// Maximum basis points value (100.00%), used both for ratings/scores and
/// for the recency-weight scale in [`recency_weight_bps`].
const BPS_SCALE: i128 = 10_000;

/// Once this many half-lives have elapsed, the recency weight is close
/// enough to zero (< 1 / 2^20 of full weight) to treat as zero outright and
/// avoid needless iteration.
const MAX_HALVINGS: u64 = 20;

/// Caps how many of an entity's most recent transactions feed the
/// time-decayed score/avg_rating computation in [`ReputationContract::recompute_score`],
/// so `record_transaction` and `rate_entity` stay bounded-cost regardless of
/// how large an entity's lifetime history grows.
const SCORE_WINDOW: u32 = 200;

/// Maps a transaction outcome to its contribution toward `score`, in basis
/// points, per the reputation score formula.
fn outcome_value_bps(outcome: &TransactionOutcome) -> i128 {
    match outcome {
        TransactionOutcome::Released => 10_000,
        TransactionOutcome::Refunded => 2_000,
        TransactionOutcome::Disputed => 0,
        TransactionOutcome::ResolvedSeller => 8_000,
        TransactionOutcome::ResolvedBuyer => 2_000,
    }
}

/// Time-decayed recency weight in basis points: `10000 * 2^(-elapsed /
/// decay_window)`, i.e. weight halves every `decay_window_secs` (the
/// formula's half-life). WASM contracts cannot use floating point, so the
/// exponential is evaluated as an exact halving for each full half-life
/// elapsed, with a linear interpolation between consecutive halvings for the
/// remainder — a deterministic fixed-point approximation of `e^(-lambda *
/// t)` accurate to a few percent, which is sufficient for reputation
/// weighting.
fn recency_weight_bps(elapsed_secs: u64, decay_window_secs: u64) -> i128 {
    if decay_window_secs == 0 {
        return BPS_SCALE;
    }
    let full_halvings = elapsed_secs / decay_window_secs;
    if full_halvings >= MAX_HALVINGS {
        return 0;
    }
    let remainder_secs = elapsed_secs % decay_window_secs;
    let base = BPS_SCALE >> full_halvings;
    if base == 0 {
        return 0;
    }
    let numerator = base * (remainder_secs as i128);
    let denominator = 2 * (decay_window_secs as i128);
    let decrement = numerator / denominator;
    (base - decrement).max(0)
}

#[contract]
pub struct ReputationContract;

#[contractimpl]
impl ReputationContract {
    // --- Initialization ---

    /// Sets the admin and config. This is a Soroban *constructor*: the host
    /// invokes it exactly once, atomically with contract deployment (see
    /// `env.register(ReputationContract, (admin, config))`), and rejects any
    /// later attempt to call it directly.
    ///
    /// A plain post-deploy `initialize(...)` function — as used elsewhere in
    /// this workspace (`escrow`, `permissions`, `delegation_registry`) —
    /// leaves a window between deployment and initialization where anyone
    /// can call it first and self-authorize as `admin`, since `admin` is
    /// itself just a caller-supplied parameter and `require_auth()` on it
    /// only proves the caller controls *some* address, not that they're the
    /// intended deployer. Making initialization part of deployment itself
    /// closes that window entirely for this contract.
    pub fn __constructor(
        env: Env,
        admin: Address,
        config: ReputationConfig,
    ) -> Result<(), ReputationError> {
        Self::validate_config(&config)?;

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Config, &config);
        Ok(())
    }

    // --- Core Recording ---

    /// Record a transaction outcome for `entity`. Called by the authorized
    /// backend/admin address (see the integration section of issue #18) once
    /// an escrow reaches `Released`, `Refunded`, `Disputed`, `ResolvedSeller`
    /// or `ResolvedBuyer`.
    ///
    /// Calling this again with an `escrow_id` already on file updates that
    /// record in place (e.g. `Disputed` followed later by `ResolvedSeller`
    /// for the same escrow) rather than appending a duplicate — the escrow's
    /// lifecycle can legitimately call this more than once, but it should
    /// only ever count once toward `total_transactions`.
    pub fn record_transaction(
        env: Env,
        caller: Address,
        escrow_id: u64,
        entity: Address,
        counterparty: Address,
        amount: i128,
        outcome: TransactionOutcome,
    ) -> Result<(), ReputationError> {
        caller.require_auth();
        let admin = Self::require_admin(&env)?;
        if caller != admin {
            return Err(ReputationError::Unauthorized);
        }

        let record_key = DataKey::TransactionRecord(escrow_id);
        let existing: Option<TransactionRecord> = env.storage().persistent().get(&record_key);
        if let Some(prior) = &existing {
            if prior.entity != entity {
                return Err(ReputationError::InvalidParam);
            }
        } else if Self::is_frozen(env.clone(), entity.clone()) {
            // Only reject brand-new escrows for a frozen entity — a
            // lifecycle update to an escrow already on file (e.g. `Disputed`
            // followed later by `ResolvedSeller`) must still be allowed to
            // land, otherwise a dispute recorded before a freeze could never
            // resolve and its penalty would outlive the freeze.
            return Err(ReputationError::EntityFrozen);
        }

        let record = TransactionRecord {
            escrow_id,
            entity: entity.clone(),
            counterparty,
            amount,
            outcome: outcome.clone(),
            rating: existing.as_ref().and_then(|r| r.rating),
            recorded_at: env.ledger().timestamp(),
        };
        env.storage().persistent().set(&record_key, &record);

        if existing.is_none() {
            let hist_key = DataKey::TransactionHistory(entity.clone());
            let mut history: Vec<u64> = env
                .storage()
                .persistent()
                .get(&hist_key)
                .unwrap_or_else(|| Vec::new(&env));
            history.push_back(escrow_id);
            env.storage().persistent().set(&hist_key, &history);
            env.storage().persistent().set(
                &DataKey::Transacted(entity.clone(), record.counterparty.clone()),
                &true,
            );
            Self::apply_new_transaction_counts(&env, &entity, &outcome);
        } else if let Some(prior) = &existing {
            Self::apply_outcome_change_counts(&env, &entity, &prior.outcome, &outcome);
        }

        let score = Self::recompute_score(&env, &entity)?;

        env.events().publish(
            (symbol_short!("reput"), symbol_short!("tx_rec")),
            TransactionRecordedEvent {
                escrow_id,
                entity,
                outcome,
                new_score: score.score,
            },
        );

        Ok(())
    }

    /// Rate the entity on the other side of a completed escrow. `rater` must
    /// be the `counterparty` recorded for `escrow_id`, and each escrow may
    /// be rated at most once (sybil resistance).
    pub fn rate_entity(
        env: Env,
        rater: Address,
        escrow_id: u64,
        entity: Address,
        rating: u32,
    ) -> Result<(), ReputationError> {
        rater.require_auth();
        if rating as i128 > BPS_SCALE {
            return Err(ReputationError::InvalidRating);
        }
        if Self::is_frozen(env.clone(), entity.clone()) {
            return Err(ReputationError::EntityFrozen);
        }

        let record_key = DataKey::TransactionRecord(escrow_id);
        let mut record: TransactionRecord = env
            .storage()
            .persistent()
            .get(&record_key)
            .ok_or(ReputationError::EntityNotFound)?;
        if record.entity != entity || record.counterparty != rater {
            return Err(ReputationError::Unauthorized);
        }
        if matches!(record.outcome, TransactionOutcome::Disputed) {
            return Err(ReputationError::InvalidParam);
        }

        let rated_key = DataKey::RatedEscrows(rater.clone());
        let mut rated: Vec<u64> = env
            .storage()
            .persistent()
            .get(&rated_key)
            .unwrap_or_else(|| Vec::new(&env));
        if rated.contains(escrow_id) {
            return Err(ReputationError::DuplicateRating);
        }
        rated.push_back(escrow_id);
        env.storage().persistent().set(&rated_key, &rated);

        record.rating = Some(rating);
        env.storage().persistent().set(&record_key, &record);

        Self::recompute_score(&env, &entity)?;

        env.events().publish(
            (symbol_short!("reput"), symbol_short!("rated")),
            EntityRatedEvent {
                rater,
                entity,
                rating,
                escrow_id,
            },
        );

        Ok(())
    }

    // --- Read-Only Views ---

    /// Returns `entity`'s reputation. `score` and `avg_rating` are masked to
    /// `0` while `total_transactions` is below
    /// `ReputationConfig::min_transactions_threshold`, per the score's
    /// public-visibility rule.
    pub fn get_reputation(env: Env, entity: Address) -> Result<ReputationScore, ReputationError> {
        let config = Self::get_config(env.clone())?;
        let mut record: ReputationScore = env
            .storage()
            .persistent()
            .get(&DataKey::Reputation(entity))
            .ok_or(ReputationError::EntityNotFound)?;
        if record.total_transactions < config.min_transactions_threshold {
            record.score = 0;
            record.avg_rating = 0;
        }
        Ok(record)
    }

    pub fn get_reputation_breakdown(
        env: Env,
        entity: Address,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<TransactionRecord>, ReputationError> {
        let history: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::TransactionHistory(entity))
            .unwrap_or_else(|| Vec::new(&env));

        let mut result = Vec::new(&env);
        let end = offset.saturating_add(limit).min(history.len());
        let mut i = offset;
        while i < end {
            let escrow_id = history.get(i).unwrap();
            if let Some(record) = env
                .storage()
                .persistent()
                .get::<DataKey, TransactionRecord>(&DataKey::TransactionRecord(escrow_id))
            {
                result.push_back(record);
            }
            i += 1;
        }
        Ok(result)
    }

    pub fn get_flags(
        env: Env,
        entity: Address,
        offset: u32,
        limit: u32,
    ) -> Result<Vec<Flag>, ReputationError> {
        let flags: Vec<Flag> = env
            .storage()
            .persistent()
            .get(&DataKey::Flags(entity))
            .unwrap_or_else(|| Vec::new(&env));

        let mut result = Vec::new(&env);
        let end = offset.saturating_add(limit).min(flags.len());
        let mut i = offset;
        while i < end {
            result.push_back(flags.get(i).unwrap());
            i += 1;
        }
        Ok(result)
    }

    pub fn is_frozen(env: Env, entity: Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::FrozenStatus(entity))
            .unwrap_or(false)
    }

    pub fn get_config(env: Env) -> Result<ReputationConfig, ReputationError> {
        env.storage()
            .instance()
            .get(&DataKey::Config)
            .ok_or(ReputationError::NotInitialized)
    }

    // --- Flagging ---

    /// Report `entity` for fraud or dispute-worthy behavior. Reporting is
    /// gated to the admin or an address that has actually transacted with
    /// `entity` (i.e. appears as `counterparty` on one of its recorded
    /// transactions) — otherwise anyone could mint free addresses and
    /// auto-freeze an arbitrary entity by reaching `freeze_threshold_flags`
    /// with throwaway reporters. A reporter may have at most one active
    /// (unresolved) flag per entity. Once the entity's active flag count
    /// reaches `ReputationConfig::freeze_threshold_flags`, it is auto-frozen.
    pub fn flag_entity(
        env: Env,
        reporter: Address,
        entity: Address,
        reason: Symbol,
        details: Option<String>,
    ) -> Result<(), ReputationError> {
        reporter.require_auth();
        let config = Self::get_config(env.clone())?;
        let admin = Self::require_admin(&env)?;
        if reporter != admin && !Self::has_transacted_with(&env, &entity, &reporter) {
            return Err(ReputationError::Unauthorized);
        }

        let key = DataKey::Flags(entity.clone());
        let mut flags: Vec<Flag> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));

        if flags.iter().any(|f| f.reporter == reporter && !f.resolved) {
            return Err(ReputationError::AlreadyFlagged);
        }

        flags.push_back(Flag {
            reporter: reporter.clone(),
            entity: entity.clone(),
            reason: reason.clone(),
            details,
            flagged_at: env.ledger().timestamp(),
            resolved: false,
        });
        env.storage().persistent().set(&key, &flags);

        let active_count = flags.iter().filter(|f| !f.resolved).count() as u32;

        env.events().publish(
            (symbol_short!("reput"), symbol_short!("flagged")),
            EntityFlaggedEvent {
                reporter,
                entity: entity.clone(),
                reason,
                flag_count: active_count,
            },
        );

        if active_count >= config.freeze_threshold_flags
            && !Self::is_frozen(env.clone(), entity.clone())
        {
            env.storage()
                .persistent()
                .set(&DataKey::FrozenStatus(entity.clone()), &true);
            env.events().publish(
                (symbol_short!("reput"), symbol_short!("frozen")),
                EntityFrozenEvent {
                    entity,
                    frozen_by: env.current_contract_address(),
                },
            );
        }

        Ok(())
    }

    /// Mark `reporter`'s flag against `entity` resolved. Admin-only. Does
    /// not automatically unfreeze — see [`Self::unfreeze_entity`].
    pub fn resolve_flag(
        env: Env,
        admin: Address,
        reporter: Address,
        entity: Address,
    ) -> Result<(), ReputationError> {
        admin.require_auth();
        Self::require_caller_is_admin(&env, &admin)?;

        let key = DataKey::Flags(entity);
        let mut flags: Vec<Flag> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Vec::new(&env));

        let idx = flags
            .iter()
            .position(|f| f.reporter == reporter && !f.resolved)
            .ok_or(ReputationError::EntityNotFound)?;
        let mut flag = flags.get(idx as u32).unwrap();
        flag.resolved = true;
        flags.set(idx as u32, flag);
        env.storage().persistent().set(&key, &flags);

        Ok(())
    }

    // --- Admin ---

    pub fn freeze_entity(env: Env, admin: Address, entity: Address) -> Result<(), ReputationError> {
        admin.require_auth();
        Self::require_caller_is_admin(&env, &admin)?;

        env.storage()
            .persistent()
            .set(&DataKey::FrozenStatus(entity.clone()), &true);
        env.events().publish(
            (symbol_short!("reput"), symbol_short!("frozen")),
            EntityFrozenEvent {
                entity,
                frozen_by: admin,
            },
        );
        Ok(())
    }

    pub fn unfreeze_entity(
        env: Env,
        admin: Address,
        entity: Address,
    ) -> Result<(), ReputationError> {
        admin.require_auth();
        Self::require_caller_is_admin(&env, &admin)?;

        env.storage()
            .persistent()
            .set(&DataKey::FrozenStatus(entity.clone()), &false);
        env.events().publish(
            (symbol_short!("reput"), symbol_short!("unfrozn")),
            EntityUnfrozenEvent {
                entity,
                unfrozen_by: admin,
            },
        );
        Ok(())
    }

    pub fn update_config(
        env: Env,
        admin: Address,
        config: ReputationConfig,
    ) -> Result<(), ReputationError> {
        admin.require_auth();
        Self::require_caller_is_admin(&env, &admin)?;
        Self::validate_config(&config)?;

        env.storage().instance().set(&DataKey::Config, &config);
        Ok(())
    }

    /// Propose a new admin. Must be called by the current admin.
    pub fn propose_admin(
        env: Env,
        current_admin: Address,
        new_admin: Address,
    ) -> Result<(), ReputationError> {
        current_admin.require_auth();
        Self::require_caller_is_admin(&env, &current_admin)?;

        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &new_admin);
        env.events().publish(
            (symbol_short!("admin"), symbol_short!("proposed")),
            AdminProposedEvent {
                current_admin,
                new_admin,
            },
        );
        Ok(())
    }

    /// Accept a proposed admin transfer. Must be called by the pending admin.
    pub fn accept_admin(env: Env, caller: Address) -> Result<(), ReputationError> {
        caller.require_auth();
        let pending: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .ok_or(ReputationError::Unauthorized)?;
        if caller != pending {
            return Err(ReputationError::Unauthorized);
        }

        env.storage().instance().set(&DataKey::Admin, &caller);
        env.storage().instance().remove(&DataKey::PendingAdmin);
        env.events().publish(
            (symbol_short!("admin"), symbol_short!("accepted")),
            AdminAcceptedEvent { new_admin: caller },
        );
        Ok(())
    }

    // --- Internal helpers ---

    fn require_admin(env: &Env) -> Result<Address, ReputationError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ReputationError::NotInitialized)
    }

    fn require_caller_is_admin(env: &Env, caller: &Address) -> Result<(), ReputationError> {
        let admin = Self::require_admin(env)?;
        if *caller != admin {
            return Err(ReputationError::Unauthorized);
        }
        Ok(())
    }

    fn validate_config(config: &ReputationConfig) -> Result<(), ReputationError> {
        if config.decay_window_seconds == 0 {
            return Err(ReputationError::InvalidParam);
        }
        if config.dispute_penalty_bps as i128 > BPS_SCALE {
            return Err(ReputationError::InvalidParam);
        }
        if config.freeze_threshold_flags == 0 {
            return Err(ReputationError::InvalidParam);
        }
        Ok(())
    }

    /// Returns `true` if `counterparty` has appeared on at least one of
    /// `entity`'s recorded transactions. Used to gate [`Self::flag_entity`]
    /// so only genuine counterparties (or the admin) can report an entity.
    ///
    /// This is an O(1) lookup against `DataKey::Transacted`, written once
    /// per new escrow in `record_transaction` — not a scan over
    /// `TransactionHistory`, which would make `flag_entity`'s cost grow with
    /// the entity's lifetime transaction count (the same unbounded-growth
    /// problem `SCORE_WINDOW` guards against in `recompute_score`).
    fn has_transacted_with(env: &Env, entity: &Address, counterparty: &Address) -> bool {
        env.storage()
            .persistent()
            .get(&DataKey::Transacted(entity.clone(), counterparty.clone()))
            .unwrap_or(false)
    }

    fn load_or_default_reputation(env: &Env, entity: &Address) -> ReputationScore {
        env.storage()
            .persistent()
            .get(&DataKey::Reputation(entity.clone()))
            .unwrap_or(ReputationScore {
                entity: entity.clone(),
                score: 0,
                total_transactions: 0,
                successful_transactions: 0,
                disputed_transactions: 0,
                avg_rating: 0,
                last_updated: 0,
            })
    }

    /// `true` for the outcomes that count toward `successful_transactions`.
    fn is_successful_outcome(outcome: &TransactionOutcome) -> bool {
        matches!(
            outcome,
            TransactionOutcome::Released | TransactionOutcome::ResolvedSeller
        )
    }

    /// Increments `entity`'s lifetime counters for a brand-new escrow.
    /// Called once per `escrow_id`, not on lifecycle updates — see
    /// [`Self::apply_outcome_change_counts`] for those.
    fn apply_new_transaction_counts(env: &Env, entity: &Address, outcome: &TransactionOutcome) {
        let mut rep = Self::load_or_default_reputation(env, entity);
        rep.total_transactions += 1;
        if Self::is_successful_outcome(outcome) {
            rep.successful_transactions += 1;
        }
        if matches!(outcome, TransactionOutcome::Disputed) {
            rep.disputed_transactions += 1;
        }
        env.storage()
            .persistent()
            .set(&DataKey::Reputation(entity.clone()), &rep);
    }

    /// Adjusts `entity`'s lifetime counters when an already-recorded escrow's
    /// outcome changes (e.g. `Disputed` -> `ResolvedSeller`), without
    /// touching `total_transactions`.
    fn apply_outcome_change_counts(
        env: &Env,
        entity: &Address,
        prior: &TransactionOutcome,
        new: &TransactionOutcome,
    ) {
        let mut rep = Self::load_or_default_reputation(env, entity);
        if Self::is_successful_outcome(prior) {
            rep.successful_transactions = rep.successful_transactions.saturating_sub(1);
        }
        if matches!(prior, TransactionOutcome::Disputed) {
            rep.disputed_transactions = rep.disputed_transactions.saturating_sub(1);
        }
        if Self::is_successful_outcome(new) {
            rep.successful_transactions += 1;
        }
        if matches!(new, TransactionOutcome::Disputed) {
            rep.disputed_transactions += 1;
        }
        env.storage()
            .persistent()
            .set(&DataKey::Reputation(entity.clone()), &rep);
    }

    /// Recomputes and persists `entity`'s `score`/`avg_rating`/
    /// `last_updated`, per the score formula:
    ///
    /// ```text
    /// score = sum(recency_weight(r) * outcome_value(r)) / sum(recency_weight(r))
    /// ```
    ///
    /// with an additional flat penalty of `dispute_penalty_bps` subtracted
    /// per still-relevant (non-fully-decayed) `Disputed` record.
    /// `avg_rating` is computed the same way over records carrying a rating.
    ///
    /// Only the most recent `SCORE_WINDOW` records feed this computation, so
    /// `record_transaction` and `rate_entity` stay bounded-cost regardless of
    /// how large an entity's lifetime history grows; records older than that
    /// already carry a recency weight close to zero for any realistic
    /// `decay_window_seconds`, so excluding them from the average has
    /// negligible effect. `total_transactions` / `successful_transactions` /
    /// `disputed_transactions` are exact lifetime counts maintained
    /// separately and incrementally — see [`Self::apply_new_transaction_counts`]
    /// and [`Self::apply_outcome_change_counts`] — so they are left as-is here.
    fn recompute_score(env: &Env, entity: &Address) -> Result<ReputationScore, ReputationError> {
        let config = Self::get_config(env.clone())?;
        let mut rep = Self::load_or_default_reputation(env, entity);

        let history: Vec<u64> = env
            .storage()
            .persistent()
            .get(&DataKey::TransactionHistory(entity.clone()))
            .unwrap_or_else(|| Vec::new(env));
        let now = env.ledger().timestamp();
        let len = history.len();
        let start = len.saturating_sub(SCORE_WINDOW);

        let mut weighted_value_sum: i128 = 0;
        let mut weight_sum: i128 = 0;
        let mut rating_weighted_sum: i128 = 0;
        let mut rating_weight_sum: i128 = 0;
        let mut disputed_recent: i128 = 0;

        let mut i = start;
        while i < len {
            let escrow_id = history.get(i).unwrap();
            i += 1;

            // A persistent entry can expire its TTL and be archived
            // independently of `TransactionHistory`; treat a missing record
            // as no longer relevant to the score rather than failing the
            // whole recomputation.
            let record: Option<TransactionRecord> = env
                .storage()
                .persistent()
                .get(&DataKey::TransactionRecord(escrow_id));
            let record = match record {
                Some(record) => record,
                None => continue,
            };

            let elapsed = now.saturating_sub(record.recorded_at);
            let weight = recency_weight_bps(elapsed, config.decay_window_seconds);
            let value = outcome_value_bps(&record.outcome);
            weighted_value_sum += weight * value;
            weight_sum += weight;

            if matches!(record.outcome, TransactionOutcome::Disputed) && weight > 0 {
                disputed_recent += 1;
            }

            if let Some(rating) = record.rating {
                rating_weighted_sum += weight * (rating as i128);
                rating_weight_sum += weight;
            }
        }

        let base_score = if weight_sum > 0 {
            weighted_value_sum / weight_sum
        } else {
            0
        };
        let penalty = disputed_recent * (config.dispute_penalty_bps as i128);
        rep.score = (base_score - penalty).clamp(0, BPS_SCALE) as u32;
        rep.avg_rating = if rating_weight_sum > 0 {
            (rating_weighted_sum / rating_weight_sum).clamp(0, BPS_SCALE) as u32
        } else {
            0
        };
        rep.last_updated = now;

        env.storage()
            .persistent()
            .set(&DataKey::Reputation(entity.clone()), &rep);
        Ok(rep)
    }
}

#[cfg(test)]
mod test;
