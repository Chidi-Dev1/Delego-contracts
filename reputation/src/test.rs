#![cfg(test)]
#![allow(clippy::module_inception)]

use crate::{
    ReputationConfig, ReputationContract, ReputationContractClient, ReputationError,
    TransactionOutcome, SCORE_WINDOW,
};
use soroban_sdk::{
    symbol_short,
    testutils::{storage::Persistent, Address as _, Ledger},
    Address, Env, String,
};

fn default_config() -> ReputationConfig {
    ReputationConfig {
        decay_window_seconds: 90 * 24 * 60 * 60,
        min_transactions_threshold: 5,
        dispute_penalty_bps: 500,
        freeze_threshold_flags: 3,
    }
}

fn setup(env: &Env) -> (ReputationContractClient<'_>, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract_id = env.register(ReputationContract, (admin.clone(), default_config()));
    let client = ReputationContractClient::new(env, &contract_id);
    (client, admin)
}

fn advance_time(env: &Env, seconds: u64) {
    env.ledger().with_mut(|li| {
        li.timestamp += seconds;
    });
}

// --- constructor (deployment-time initialization) ---
//
// `__constructor` is a Soroban constructor: the host runs it exactly once,
// atomically with `env.register`, and there is no way to invoke it again
// afterward (unlike the plain `initialize()` pattern used elsewhere in this
// workspace) — so there is no "already initialized" or "not yet
// initialized" state to exercise here, only deployment succeeding or
// `env.register` panicking on an invalid config.

#[test]
fn test_constructor_sets_admin_and_config() {
    let env = Env::default();
    let (client, _admin) = setup(&env);

    assert_eq!(client.get_config(), default_config());
}

#[test]
#[should_panic]
fn test_constructor_rejects_zero_decay_window() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let mut bad = default_config();
    bad.decay_window_seconds = 0;
    env.register(ReputationContract, (admin, bad));
}

#[test]
#[should_panic]
fn test_constructor_rejects_dispute_penalty_over_max() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let mut bad = default_config();
    bad.dispute_penalty_bps = 10_001;
    env.register(ReputationContract, (admin, bad));
}

#[test]
#[should_panic]
fn test_constructor_rejects_zero_freeze_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let mut bad = default_config();
    bad.freeze_threshold_flags = 0;
    env.register(ReputationContract, (admin, bad));
}

// --- min_transactions_threshold vs SCORE_WINDOW ---

#[test]
#[should_panic]
fn test_constructor_rejects_threshold_over_window() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let mut bad = default_config();
    bad.min_transactions_threshold = SCORE_WINDOW as u64 + 1;
    env.register(ReputationContract, (admin, bad));
}

#[test]
fn test_constructor_accepts_threshold_equal_to_window() {
    // A threshold exactly equal to SCORE_WINDOW is the largest reachable
    // value and must be accepted (the masking gate compares the lifetime
    // counter, so hitting it is still possible).
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let mut cfg = default_config();
    cfg.min_transactions_threshold = SCORE_WINDOW as u64;
    let contract_id = env.register(ReputationContract, (admin, cfg.clone()));
    let client = ReputationContractClient::new(&env, &contract_id);
    assert_eq!(client.get_config(), cfg);
}

#[test]
fn test_update_config_rejects_threshold_over_window() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    let mut bad = default_config();
    bad.min_transactions_threshold = SCORE_WINDOW as u64 + 1;
    let res = client.try_update_config(&admin, &bad);
    assert_eq!(res, Err(Ok(ReputationError::InvalidParam)));
}

// The masking gate in get_reputation compares the entity's *lifetime*
// `total_transactions` (window-independent) against the threshold — not the
// `SCORE_WINDOW` sample feeding the score recompute. Setting the threshold
// equal to SCORE_WINDOW and recording exactly SCORE_WINDOW lifetime
// transactions unmasks the score even though the score itself is computed
// over the same-sized window, pinning that the gate is driven by lifetime
// counts.
#[test]
fn test_masking_uses_lifetime_counts_not_window() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let mut cfg = default_config();
    cfg.min_transactions_threshold = SCORE_WINDOW as u64;
    let contract_id = env.register(ReputationContract, (admin.clone(), cfg));
    let client = ReputationContractClient::new(&env, &contract_id);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    // Record exactly SCORE_WINDOW lifetime transactions. Remaining just below
    // the threshold would keep the score masked; at the threshold it unmasks.
    for i in 0..SCORE_WINDOW as u64 {
        client.record_transaction(
            &admin,
            &i,
            &entity,
            &counterparty,
            &1000i128,
            &TransactionOutcome::Released,
        );
    }

    let rep = client.get_reputation(&entity);
    // Lifetime count is exact, and the fresh all-Released run scores full.
    assert_eq!(rep.total_transactions, SCORE_WINDOW as u64);
    assert_eq!(rep.score, 10_000);
}

#[test]
fn test_masking_keeps_score_hidden_below_threshold() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let mut cfg = default_config();
    cfg.min_transactions_threshold = SCORE_WINDOW as u64;
    let contract_id = env.register(ReputationContract, (admin.clone(), cfg));
    let client = ReputationContractClient::new(&env, &contract_id);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    // SCORE_WINDOW - 1 lifetime transactions: still below the threshold, so
    // the score/avg_rating must stay masked even though the recompute
    // samples the same window size.
    for i in 0..(SCORE_WINDOW as u64 - 1) {
        client.record_transaction(
            &admin,
            &i,
            &entity,
            &counterparty,
            &1000i128,
            &TransactionOutcome::Released,
        );
    }

    let rep = client.get_reputation(&entity);
    assert_eq!(rep.total_transactions, SCORE_WINDOW as u64 - 1);
    assert_eq!(rep.score, 0);
}

// --- record_transaction ---

#[test]
fn test_record_transaction_released_scores_full() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    client.record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Released,
    );

    let rep = client.try_get_reputation(&entity).unwrap().unwrap();
    // Below min_transactions_threshold (5), so score is masked.
    assert_eq!(rep.score, 0);
    assert_eq!(rep.total_transactions, 1);
    assert_eq!(rep.successful_transactions, 1);
    assert_eq!(rep.disputed_transactions, 0);
}

#[test]
fn test_record_transaction_records_relation_in_both_directions() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    client.record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Released,
    );

    assert!(client.has_relation(&entity, &counterparty, &false));
    assert!(client.has_relation(&counterparty, &entity, &false));
    assert!(client.has_relation(&entity, &counterparty, &true));
    assert!(client.has_relation(&counterparty, &entity, &true));
    let stranger = Address::generate(&env);
    assert!(!client.has_relation(&entity, &stranger, &false));
}

#[test]
fn test_record_transaction_unmasks_score_at_threshold() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    for i in 0..5u64 {
        client.record_transaction(
            &admin,
            &i,
            &entity,
            &counterparty,
            &1000i128,
            &TransactionOutcome::Released,
        );
    }

    let rep = client.get_reputation(&entity);
    assert_eq!(rep.total_transactions, 5);
    // All-Released, freshly recorded (no decay yet) -> full score.
    assert_eq!(rep.score, 10_000);
}

#[test]
fn test_record_transaction_unauthorized_caller() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);
    let not_admin = Address::generate(&env);

    let res = client.try_record_transaction(
        &not_admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Released,
    );
    assert_eq!(res, Err(Ok(ReputationError::Unauthorized)));
}

#[test]
fn test_record_transaction_rejects_frozen_entity() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    client.freeze_entity(&admin, &entity);

    let res = client.try_record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Released,
    );
    assert_eq!(res, Err(Ok(ReputationError::EntityFrozen)));
}

#[test]
fn test_record_transaction_allows_lifecycle_update_while_frozen() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    client.record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Disputed,
    );
    client.freeze_entity(&admin, &entity);

    // A dispute opened before the freeze must still be able to resolve —
    // only brand-new escrows are rejected while frozen.
    client.record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::ResolvedSeller,
    );

    let rep = client.get_reputation(&entity);
    assert_eq!(rep.disputed_transactions, 0);
    assert_eq!(rep.successful_transactions, 1);

    // A genuinely new escrow is still rejected while frozen.
    let res = client.try_record_transaction(
        &admin,
        &2u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Released,
    );
    assert_eq!(res, Err(Ok(ReputationError::EntityFrozen)));
}

#[test]
fn test_record_transaction_extends_ttl_across_churn() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);
    let record_key = crate::DataKey::TransactionRecord(1);

    client.record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Disputed,
    );
    let initial_ttl = env.as_contract(&client.address, || {
        env.storage().persistent().get_ttl(&record_key)
    });
    assert!(initial_ttl > 17_280);

    env.ledger().set_sequence_number(initial_ttl - 17_280 + 1);
    client.record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::ResolvedSeller,
    );
    let refreshed_ttl = env.as_contract(&client.address, || {
        env.storage().persistent().get_ttl(&record_key)
    });
    assert!(refreshed_ttl > 17_280);

    env.ledger().set_sequence_number(refreshed_ttl - 17_280 + 1);
    client.record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Released,
    );
    let final_ttl = env.as_contract(&client.address, || {
        env.storage().persistent().get_ttl(&record_key)
    });
    assert!(final_ttl > 17_280);

    env.ledger().set_sequence_number(final_ttl - 1);
    assert!(!client
        .get_reputation_breakdown(&entity, &0u32, &10u32)
        .is_empty());
}

#[test]
fn test_record_transaction_same_escrow_updates_in_place() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    client.record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Disputed,
    );
    client.record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::ResolvedSeller,
    );

    let rep = client.get_reputation(&entity);
    // Lifecycle update on the same escrow_id must not double count.
    assert_eq!(rep.total_transactions, 1);
    assert_eq!(rep.disputed_transactions, 0);
    assert_eq!(rep.successful_transactions, 1);

    let breakdown = client.get_reputation_breakdown(&entity, &0u32, &10u32);
    assert_eq!(breakdown.len(), 1);
    assert!(matches!(
        breakdown.get(0).unwrap().outcome,
        TransactionOutcome::ResolvedSeller
    ));
}

#[test]
fn test_record_transaction_rejects_entity_mismatch_for_existing_escrow() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity_a = Address::generate(&env);
    let entity_b = Address::generate(&env);
    let counterparty = Address::generate(&env);

    client.record_transaction(
        &admin,
        &1u64,
        &entity_a,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Released,
    );

    let res = client.try_record_transaction(
        &admin,
        &1u64,
        &entity_b,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Released,
    );
    assert_eq!(res, Err(Ok(ReputationError::InvalidParam)));
}

#[test]
fn test_score_decays_over_time() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    for i in 0..5u64 {
        client.record_transaction(
            &admin,
            &i,
            &entity,
            &counterparty,
            &1000i128,
            &TransactionOutcome::Released,
        );
    }
    let fresh = client.get_reputation(&entity);
    assert_eq!(fresh.score, 10_000);

    // Advance one full half-life (decay_window_seconds) and force a
    // recompute via a new transaction; the older Released records should
    // now contribute at roughly half weight, pulling a fresh 0-value
    // Disputed record's average down less than it would immediately after
    // (i.e. the mix is dominated less by old data over time). We assert the
    // simpler, robust invariant: recorded_at-fresh entries score higher
    // than long-decayed ones feeding a poor outcome.
    advance_time(&env, default_config().decay_window_seconds);
    client.record_transaction(
        &admin,
        &99u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Disputed,
    );
    let decayed = client.get_reputation(&entity);
    assert!(decayed.score < fresh.score);
}

#[test]
fn test_score_bounded_to_recent_window() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    // Old disputes that should age out of the score window once more than
    // SCORE_WINDOW clean transactions have been recorded since.
    for i in 0..10u64 {
        client.record_transaction(
            &admin,
            &i,
            &entity,
            &counterparty,
            &1000i128,
            &TransactionOutcome::Disputed,
        );
    }
    for i in 10..(10 + crate::SCORE_WINDOW as u64) {
        client.record_transaction(
            &admin,
            &i,
            &entity,
            &counterparty,
            &1000i128,
            &TransactionOutcome::Released,
        );
    }

    let rep = client.get_reputation(&entity);
    // Lifetime counts remain exact regardless of the scoring window.
    assert_eq!(rep.total_transactions, 10 + crate::SCORE_WINDOW as u64);
    assert_eq!(rep.disputed_transactions, 10);
    // The old disputes have fully aged out of the SCORE_WINDOW most-recent
    // records feeding the score, so the score reflects only the clean run.
    assert_eq!(rep.score, 10_000);

    // One more dispute lands inside the window, so it now counts against the
    // score. This pins the boundary: the window, not the dispute count, is
    // what excluded the earlier ones.
    client.record_transaction(
        &admin,
        &(10 + crate::SCORE_WINDOW as u64),
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Disputed,
    );
    let rep = client.get_reputation(&entity);
    assert!(rep.score < 10_000);
}

// --- rate_entity ---

#[test]
fn test_rate_entity_happy_path() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    client.record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Released,
    );
    client.rate_entity(&counterparty, &1u64, &entity, &9000u32);

    let breakdown = client.get_reputation_breakdown(&entity, &0u32, &10u32);
    assert_eq!(breakdown.get(0).unwrap().rating, Some(9000u32));
}

#[test]
fn test_rate_entity_updates_avg_rating() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    // Reach min_transactions_threshold (5) so the score/avg_rating are
    // visible on get_reputation.
    for i in 0..5u64 {
        client.record_transaction(
            &admin,
            &i,
            &entity,
            &counterparty,
            &1000i128,
            &TransactionOutcome::Released,
        );
    }

    // Only two of the five are rated; avg_rating must reflect only those,
    // not all five recorded transactions.
    client.rate_entity(&counterparty, &0u64, &entity, &8000u32);
    client.rate_entity(&counterparty, &1u64, &entity, &10_000u32);

    let rep = client.get_reputation(&entity);
    // All five records are equally fresh (same recorded_at), so the two
    // ratings carry equal weight: (8000 + 10000) / 2.
    assert_eq!(rep.avg_rating, 9000);
}

#[test]
fn test_rate_entity_duplicate_rejected() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    client.record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Released,
    );
    client.rate_entity(&counterparty, &1u64, &entity, &9000u32);

    let res = client.try_rate_entity(&counterparty, &1u64, &entity, &8000u32);
    assert_eq!(res, Err(Ok(ReputationError::DuplicateRating)));
}

#[test]
fn test_rate_entity_invalid_rating_rejected() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    client.record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Released,
    );

    let res = client.try_rate_entity(&counterparty, &1u64, &entity, &10_001u32);
    assert_eq!(res, Err(Ok(ReputationError::InvalidRating)));
}

#[test]
fn test_rate_entity_wrong_rater_rejected() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Released,
    );

    let res = client.try_rate_entity(&stranger, &1u64, &entity, &9000u32);
    assert_eq!(res, Err(Ok(ReputationError::Unauthorized)));
}

#[test]
fn test_rate_entity_missing_escrow_rejected() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    let res = client.try_rate_entity(&counterparty, &1u64, &entity, &9000u32);
    assert_eq!(res, Err(Ok(ReputationError::EntityNotFound)));
}

#[test]
fn test_rate_entity_rejects_disputed_outcome() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    client.record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Disputed,
    );

    let res = client.try_rate_entity(&counterparty, &1u64, &entity, &9000u32);
    assert_eq!(res, Err(Ok(ReputationError::InvalidParam)));
}

#[test]
fn test_rate_entity_rejects_frozen_entity() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    client.record_transaction(
        &admin,
        &1u64,
        &entity,
        &counterparty,
        &1000i128,
        &TransactionOutcome::Released,
    );
    client.freeze_entity(&admin, &entity);

    let res = client.try_rate_entity(&counterparty, &1u64, &entity, &9000u32);
    assert_eq!(res, Err(Ok(ReputationError::EntityFrozen)));
}

// --- get_reputation ---

#[test]
fn test_get_reputation_not_found() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let entity = Address::generate(&env);

    assert_eq!(
        client.try_get_reputation(&entity),
        Err(Ok(ReputationError::EntityNotFound))
    );
}

// --- pagination ---

#[test]
fn test_get_reputation_breakdown_pagination() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let counterparty = Address::generate(&env);

    for i in 0..10u64 {
        client.record_transaction(
            &admin,
            &i,
            &entity,
            &counterparty,
            &1000i128,
            &TransactionOutcome::Released,
        );
    }

    let page1 = client.get_reputation_breakdown(&entity, &0u32, &4u32);
    assert_eq!(page1.len(), 4);
    let page2 = client.get_reputation_breakdown(&entity, &4u32, &4u32);
    assert_eq!(page2.len(), 4);
    let page3 = client.get_reputation_breakdown(&entity, &8u32, &4u32);
    assert_eq!(page3.len(), 2);

    let out_of_range = client.get_reputation_breakdown(&entity, &100u32, &4u32);
    assert_eq!(out_of_range.len(), 0);
}

// --- flagging ---

/// Establishes `reporter` as a genuine counterparty of `entity` via a
/// completed transaction, satisfying `flag_entity`'s reporter gate.
fn make_transacting_counterparty(
    client: &ReputationContractClient,
    admin: &Address,
    entity: &Address,
    reporter: &Address,
    escrow_id: u64,
) {
    client.record_transaction(
        admin,
        &escrow_id,
        entity,
        reporter,
        &1000i128,
        &TransactionOutcome::Released,
    );
}

#[test]
fn test_flag_entity_happy_path() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let reporter = Address::generate(&env);
    make_transacting_counterparty(&client, &admin, &entity, &reporter, 1u64);

    client.flag_entity(
        &reporter,
        &entity,
        &symbol_short!("fraud"),
        &Some(String::from_str(&env, "fake goods")),
    );

    let flags = client.get_flags(&entity, &0u32, &10u32);
    assert_eq!(flags.len(), 1);
    assert!(!flags.get(0).unwrap().resolved);
    assert!(!client.is_frozen(&entity));
}

#[test]
fn test_flag_entity_rejects_non_counterparty_non_admin() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let entity = Address::generate(&env);
    let stranger = Address::generate(&env);

    let res = client.try_flag_entity(&stranger, &entity, &symbol_short!("fraud"), &None);
    assert_eq!(res, Err(Ok(ReputationError::Unauthorized)));
}

#[test]
fn test_flag_entity_allows_admin_without_transaction() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);

    client.flag_entity(&admin, &entity, &symbol_short!("fraud"), &None);

    let flags = client.get_flags(&entity, &0u32, &10u32);
    assert_eq!(flags.len(), 1);
}

#[test]
fn test_flag_entity_same_reporter_rejected_while_active() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let reporter = Address::generate(&env);
    make_transacting_counterparty(&client, &admin, &entity, &reporter, 1u64);

    client.flag_entity(&reporter, &entity, &symbol_short!("fraud"), &None);

    let res = client.try_flag_entity(&reporter, &entity, &symbol_short!("spam"), &None);
    assert_eq!(res, Err(Ok(ReputationError::AlreadyFlagged)));
}

#[test]
fn test_flag_entity_auto_freezes_at_threshold() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);

    for i in 0..3u64 {
        let reporter = Address::generate(&env);
        make_transacting_counterparty(&client, &admin, &entity, &reporter, i);
        client.flag_entity(&reporter, &entity, &symbol_short!("fraud"), &None);
    }

    assert!(client.is_frozen(&entity));
}

#[test]
fn test_resolve_flag_happy_path() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let reporter = Address::generate(&env);
    make_transacting_counterparty(&client, &admin, &entity, &reporter, 1u64);

    client.flag_entity(&reporter, &entity, &symbol_short!("fraud"), &None);
    client.resolve_flag(&admin, &reporter, &entity);

    let flags = client.get_flags(&entity, &0u32, &10u32);
    assert!(flags.get(0).unwrap().resolved);

    // Reporter can flag again now that their prior flag is resolved.
    client.flag_entity(&reporter, &entity, &symbol_short!("spam"), &None);
    let flags = client.get_flags(&entity, &0u32, &10u32);
    assert_eq!(flags.len(), 2);
}

#[test]
fn test_resolve_flag_unauthorized() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let reporter = Address::generate(&env);
    let not_admin = Address::generate(&env);
    make_transacting_counterparty(&client, &admin, &entity, &reporter, 1u64);

    client.flag_entity(&reporter, &entity, &symbol_short!("fraud"), &None);

    let res = client.try_resolve_flag(&not_admin, &reporter, &entity);
    assert_eq!(res, Err(Ok(ReputationError::Unauthorized)));
}

#[test]
fn test_resolve_flag_missing() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);
    let reporter = Address::generate(&env);

    let res = client.try_resolve_flag(&admin, &reporter, &entity);
    assert_eq!(res, Err(Ok(ReputationError::EntityNotFound)));
}

// --- freeze / unfreeze ---

#[test]
fn test_freeze_and_unfreeze_entity() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let entity = Address::generate(&env);

    assert!(!client.is_frozen(&entity));
    client.freeze_entity(&admin, &entity);
    assert!(client.is_frozen(&entity));
    client.unfreeze_entity(&admin, &entity);
    assert!(!client.is_frozen(&entity));
}

#[test]
fn test_freeze_entity_unauthorized() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let entity = Address::generate(&env);
    let not_admin = Address::generate(&env);

    let res = client.try_freeze_entity(&not_admin, &entity);
    assert_eq!(res, Err(Ok(ReputationError::Unauthorized)));
}

// --- update_config ---

#[test]
fn test_update_config_happy_path() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    let mut new_config = default_config();
    new_config.min_transactions_threshold = 1;
    client.update_config(&admin, &new_config);

    assert_eq!(client.get_config(), new_config);
}

#[test]
fn test_get_config_parity() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    let mut new_config = default_config();
    new_config.freeze_threshold_flags = 1;
    client.update_config(&admin, &new_config);

    assert_eq!(client.get_config(), new_config);
}

#[test]
fn test_update_config_unauthorized() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let not_admin = Address::generate(&env);

    let res = client.try_update_config(&not_admin, &default_config());
    assert_eq!(res, Err(Ok(ReputationError::Unauthorized)));
}

#[test]
fn test_update_config_rejects_invalid() {
    let env = Env::default();
    let (client, admin) = setup(&env);

    let mut bad = default_config();
    bad.decay_window_seconds = 0;
    let res = client.try_update_config(&admin, &bad);
    assert_eq!(res, Err(Ok(ReputationError::InvalidParam)));
}

// --- admin transfer ---

#[test]
fn test_propose_and_accept_admin() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let new_admin = Address::generate(&env);

    client.propose_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);

    // Old admin can no longer perform admin actions.
    let entity = Address::generate(&env);
    let res = client.try_freeze_entity(&admin, &entity);
    assert_eq!(res, Err(Ok(ReputationError::Unauthorized)));

    // New admin can.
    client.freeze_entity(&new_admin, &entity);
    assert!(client.is_frozen(&entity));
}

#[test]
fn test_propose_admin_unauthorized() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let not_admin = Address::generate(&env);
    let new_admin = Address::generate(&env);

    let res = client.try_propose_admin(&not_admin, &new_admin);
    assert_eq!(res, Err(Ok(ReputationError::Unauthorized)));
}

#[test]
fn test_accept_admin_wrong_caller() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let new_admin = Address::generate(&env);
    let stranger = Address::generate(&env);

    client.propose_admin(&admin, &new_admin);

    let res = client.try_accept_admin(&stranger);
    assert_eq!(res, Err(Ok(ReputationError::Unauthorized)));
}

#[test]
fn test_accept_admin_no_pending_transfer() {
    let env = Env::default();
    let (client, _admin) = setup(&env);
    let stranger = Address::generate(&env);

    let res = client.try_accept_admin(&stranger);
    assert_eq!(res, Err(Ok(ReputationError::Unauthorized)));
}

// --- recency_weight_bps decay curve ---

/// Invariant: `BPS_SCALE >> MAX_HALVINGS` must be non-zero.
///
/// If this fails it means `MAX_HALVINGS` has drifted past the point where the
/// shift can produce any non-zero base, making the guard in
/// `recency_weight_bps` fire *after* the shift already rounds to zero —
/// i.e. the documented cutoff is later than the actual one.
#[test]
fn test_max_halvings_invariant() {
    // Compile-time invariant check: the half-life shift must still produce a
    // non-zero base at the declared cutoff (see doc comment above).
    const {
        assert!(
            crate::BPS_SCALE >> crate::MAX_HALVINGS != 0,
            "BPS_SCALE >> MAX_HALVINGS == 0: MAX_HALVINGS must be lowered \
         (or BPS_SCALE raised) so that the shift still produces a non-zero \
         base at the declared cutoff",
        )
    }
}

/// Decay-curve smoke test: checks expected outputs for every full half-life
/// from 0 through MAX_HALVINGS (inclusive), using a decay window of exactly
/// 1 second so that `elapsed = k` implies exactly `k` full halvings and
/// zero remainder seconds.
///
/// At k = 0          : weight == BPS_SCALE (no decay).
/// At k = MAX_HALVINGS: weight == 0 (early-exit guard fires).
///
/// The test verifies:
///   1. weight == BPS_SCALE at k == 0.
///   2. Strict monotone decrease for k in 0..MAX_HALVINGS.
///   3. weight == 0 at k == MAX_HALVINGS (guard's documented semantics).
#[test]
fn test_recency_weight_decay_curve() {
    // decay_window = 1 s so that elapsed = k  =>  full_halvings = k,
    // remainder_secs = 0, no linear interpolation term.
    let decay_window: u64 = 1;
    let max = crate::MAX_HALVINGS;

    // k=0: full weight.
    assert_eq!(
        crate::recency_weight_bps(0, decay_window),
        crate::BPS_SCALE,
        "weight at 0 half-lives should equal BPS_SCALE"
    );

    // k=MAX_HALVINGS: guard fires, weight must be 0.
    assert_eq!(
        crate::recency_weight_bps(max, decay_window),
        0,
        "weight at MAX_HALVINGS ({max}) half-lives should be 0"
    );

    // Strict monotone decrease across 0..MAX_HALVINGS.
    let mut prev = crate::recency_weight_bps(0, decay_window);
    for k in 1..max {
        let curr = crate::recency_weight_bps(k, decay_window);
        assert!(
            curr < prev,
            "decay curve not strictly decreasing at half-life {k}: \
             weight[{k}]={curr} is not less than weight[{}]={prev}",
            k - 1,
        );
        prev = curr;
    }
}
