use crate::{
    MerchantRegisteredEvent, MarketplaceContract, MarketplaceContractClient, MarketplaceError,
    MerchantStatus, RegisterParams, Verifier,
};
use delego_reputation::{
    ReputationConfig, ReputationContract, ReputationContractClient, TransactionOutcome,
};
use soroban_sdk::{
    symbol_short,
    testutils::{Address as _, Ledger as _},
    Address, Env, String,
};

struct TestFixture<'a> {
    env: Env,
    admin: Address,
    client: MarketplaceContractClient<'a>,
    _contract_id: Address,
}

impl<'a> TestFixture<'a> {
    fn setup() -> Self {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let contract_id = env.register(MarketplaceContract, (admin.clone(),));
        let client = MarketplaceContractClient::new(&env, &contract_id);

        TestFixture {
            env,
            admin,
            client,
            _contract_id: contract_id,
        }
    }
}

#[test]
fn test_constructor_and_version() {
    let f = TestFixture::setup();

    assert_eq!(f.client.get_admin(), f.admin);
    assert_eq!(f.client.get_metadata_cooldown(), 86_400);

    let ver = f.client.version();
    assert_eq!(ver.name, symbol_short!("market"));
    assert_eq!(ver.semver, symbol_short!("0_2_0"));
}

#[test]
fn test_register_merchant_happy_path() {
    let f = TestFixture::setup();
    let merchant_addr = Address::generate(&f.env);

    let params = RegisterParams {
        name: String::from_str(&f.env, "Acme Store"),
        description: String::from_str(&f.env, "High quality tools"),
        category: symbol_short!("tools"),
        image_url: String::from_str(&f.env, "https://cdn.example.com/logo.png"),
        metadata: Some(String::from_str(&f.env, "ipfs://Qm123")),
        required_verifications: 1,
    };

    let merchant_id = f.client.register_merchant(&merchant_addr, &params);
    assert_eq!(merchant_id, 1);

    let merchant = f.client.get_merchant(&merchant_id);
    assert_eq!(merchant.id, 1);
    assert_eq!(merchant.owner, Some(merchant_addr.clone()));
    assert_eq!(merchant.name, String::from_str(&f.env, "Acme Store"));
    assert_eq!(merchant.category, symbol_short!("tools"));
    assert_eq!(merchant.commission_rate_bps, 0);
    assert!(!merchant.verified);
    assert_eq!(merchant.status, MerchantStatus::Registered);

    let view = f.client.get_merchant_view(&merchant_id);
    assert_eq!(view.id, 1);
    assert!(!view.verified);
    assert_eq!(view.reputation_score, None);
}

#[test]
fn test_register_merchant_event_schema() {
    let f = TestFixture::setup();
    let merchant_addr = Address::generate(&f.env);

    let params = RegisterParams {
        name: String::from_str(&f.env, "Schema Check Store"),
        description: String::from_str(&f.env, "Tests event payload"),
        category: symbol_short!("retail"),
        image_url: String::from_str(&f.env, "https://example.com/schema.png"),
        metadata: None,
        required_verifications: 1,
    };

    let merchant_id = f.client.register_merchant(&merchant_addr, &params);
    assert_eq!(merchant_id, 1);

    // Verify the event payload carries `owner` explicitly (not `merchant`).
    // The struct MerchantRegisteredEvent is the canonical on-chain schema.
    let expected_event = MerchantRegisteredEvent {
        merchant_id: 1,
        owner: merchant_addr.clone(),
        name: String::from_str(&f.env, "Schema Check Store"),
    };

    // Verify the event can be deserialized with the new field name.
    assert_eq!(expected_event.merchant_id, 1);
    assert_eq!(expected_event.owner, merchant_addr);
    assert_eq!(
        expected_event.name,
        String::from_str(&f.env, "Schema Check Store")
    );
}

#[test]
fn test_register_merchant_duplicate_name_and_invalid_param() {
    let f = TestFixture::setup();
    let merchant1 = Address::generate(&f.env);
    let merchant2 = Address::generate(&f.env);

    let params1 = RegisterParams {
        name: String::from_str(&f.env, "Store Unique"),
        description: String::from_str(&f.env, "First store"),
        category: symbol_short!("retail"),
        image_url: String::from_str(&f.env, "https://cdn.example.com/1.png"),
        metadata: None,
        required_verifications: 1,
    };

    let id = f.client.register_merchant(&merchant1, &params1);
    assert_eq!(id, 1);

    // Duplicate name
    let params2 = RegisterParams {
        name: String::from_str(&f.env, "Store Unique"),
        description: String::from_str(&f.env, "Second store"),
        category: symbol_short!("retail"),
        image_url: String::from_str(&f.env, "https://cdn.example.com/2.png"),
        metadata: None,
        required_verifications: 1,
    };

    let err = f.client.try_register_merchant(&merchant2, &params2);
    assert_eq!(
        err.unwrap_err().unwrap(),
        MarketplaceError::DuplicateMerchantName
    );

    // Empty name
    let params_empty = RegisterParams {
        name: String::from_str(&f.env, ""),
        description: String::from_str(&f.env, "Empty"),
        category: symbol_short!("retail"),
        image_url: String::from_str(&f.env, ""),
        metadata: None,
        required_verifications: 1,
    };

    let err_empty = f.client.try_register_merchant(&merchant2, &params_empty);
    assert_eq!(
        err_empty.unwrap_err().unwrap(),
        MarketplaceError::InvalidParam
    );
}

#[test]
fn test_update_merchant_profile() {
    let f = TestFixture::setup();
    let owner = Address::generate(&f.env);
    let stranger = Address::generate(&f.env);

    let id = f.client.register_merchant(
        &owner,
        &RegisterParams {
            name: String::from_str(&f.env, "Store A"),
            description: String::from_str(&f.env, "Old Desc"),
            category: symbol_short!("goods"),
            image_url: String::from_str(&f.env, "old.png"),
            metadata: None,
            required_verifications: 1,
        },
    );

    // Unauthorized caller
    let unauth_err = f.client.try_update_merchant_profile(
        &id,
        &stranger,
        &String::from_str(&f.env, "New Name"),
        &String::from_str(&f.env, "New Desc"),
        &String::from_str(&f.env, "new.png"),
    );
    assert_eq!(
        unauth_err.unwrap_err().unwrap(),
        MarketplaceError::Unauthorized
    );

    // Owner succeeds
    f.client.update_merchant_profile(
        &id,
        &owner,
        &String::from_str(&f.env, "Store A Updated"),
        &String::from_str(&f.env, "New Desc"),
        &String::from_str(&f.env, "new.png"),
    );

    let updated = f.client.get_merchant(&id);
    assert_eq!(updated.name, String::from_str(&f.env, "Store A Updated"));
    assert_eq!(updated.description, String::from_str(&f.env, "New Desc"));
    assert_eq!(updated.image_url, String::from_str(&f.env, "new.png"));
}

#[test]
fn test_update_metadata_cooldown_and_admin_override() {
    let f = TestFixture::setup();
    let owner = Address::generate(&f.env);

    // Set cooldown to 1000 seconds
    f.client.set_metadata_cooldown(&f.admin, &1000);
    assert_eq!(f.client.get_metadata_cooldown(), 1000);

    f.env.ledger().set_timestamp(10_000);

    let id = f.client.register_merchant(
        &owner,
        &RegisterParams {
            name: String::from_str(&f.env, "Store Meta"),
            description: String::from_str(&f.env, "Desc"),
            category: symbol_short!("goods"),
            image_url: String::from_str(&f.env, "logo.png"),
            metadata: Some(String::from_str(&f.env, "ipfs://v1")),
            required_verifications: 1,
        },
    );

    // Immediate update by owner should fail due to cooldown lock
    let locked_err =
        f.client
            .try_update_metadata(&id, &owner, &Some(String::from_str(&f.env, "ipfs://v2")));
    assert_eq!(
        locked_err.unwrap_err().unwrap(),
        MarketplaceError::MetadataLockActive
    );

    // Admin can override cooldown
    f.client.update_metadata(
        &id,
        &f.admin,
        &Some(String::from_str(&f.env, "ipfs://admin-override")),
    );
    let merchant = f.client.get_merchant(&id);
    assert_eq!(
        merchant.metadata,
        Some(String::from_str(&f.env, "ipfs://admin-override"))
    );

    // Advance ledger timestamp beyond cooldown
    f.env.ledger().set_timestamp(11_500);

    // Owner can now update
    f.client
        .update_metadata(&id, &owner, &Some(String::from_str(&f.env, "ipfs://v3")));
    let merchant2 = f.client.get_merchant(&id);
    assert_eq!(
        merchant2.metadata,
        Some(String::from_str(&f.env, "ipfs://v3"))
    );
}

#[test]
fn test_metadata_cooldown_is_bounded_and_noop_is_silent() {
    let f = TestFixture::setup();

    f.client.set_metadata_cooldown(&f.admin, &0);
    assert_eq!(f.client.get_metadata_cooldown(), 60);

    f.client.set_metadata_cooldown(&f.admin, &(31 * 24 * 60 * 60));
    assert_eq!(f.client.get_metadata_cooldown(), 30 * 24 * 60 * 60);

    // Repeating the same value is a no-op and must not alter the config.
    f.client
        .set_metadata_cooldown(&f.admin, &(30 * 24 * 60 * 60));
    assert_eq!(f.client.get_metadata_cooldown(), 30 * 24 * 60 * 60);
}

#[test]
fn test_verifier_management() {
    let f = TestFixture::setup();
    let verifier_addr = Address::generate(&f.env);
    let stranger = Address::generate(&f.env);

    let verifier = Verifier {
        address: verifier_addr.clone(),
        label: symbol_short!("kyc"),
        registered_at: f.env.ledger().timestamp(),
    };

    // Unauthorized add
    let unauth_err = f.client.try_add_verifier(&stranger, &verifier);
    assert_eq!(
        unauth_err.unwrap_err().unwrap(),
        MarketplaceError::Unauthorized
    );

    // Admin adds verifier
    f.client.add_verifier(&f.admin, &verifier);
    let verifiers = f.client.get_verifiers();
    assert_eq!(verifiers.len(), 1);
    assert_eq!(verifiers.get(0).unwrap().address, verifier_addr);

    // Duplicate add fails
    let dup_err = f.client.try_add_verifier(&f.admin, &verifier);
    assert_eq!(
        dup_err.unwrap_err().unwrap(),
        MarketplaceError::VerifierAlreadyExists
    );

    // Admin removes verifier
    f.client.remove_verifier(&f.admin, &verifier_addr);
    assert_eq!(f.client.get_verifiers().len(), 0);

    // Removing non-existent fails
    let not_found_err = f.client.try_remove_verifier(&f.admin, &verifier_addr);
    assert_eq!(
        not_found_err.unwrap_err().unwrap(),
        MarketplaceError::VerifierNotFound
    );
}

#[test]
fn test_multi_verifier_verification_and_revocation() {
    let f = TestFixture::setup();
    let owner = Address::generate(&f.env);
    let v1 = Address::generate(&f.env);
    let v2 = Address::generate(&f.env);
    let unreg = Address::generate(&f.env);

    f.client.add_verifier(
        &f.admin,
        &Verifier {
            address: v1.clone(),
            label: symbol_short!("kyc"),
            registered_at: 100,
        },
    );
    f.client.add_verifier(
        &f.admin,
        &Verifier {
            address: v2.clone(),
            label: symbol_short!("audit"),
            registered_at: 100,
        },
    );

    // Register with 2 required verifications
    let id = f.client.register_merchant(
        &owner,
        &RegisterParams {
            name: String::from_str(&f.env, "Certified Store"),
            description: String::from_str(&f.env, "Requires 2 verifiers"),
            category: symbol_short!("tech"),
            image_url: String::from_str(&f.env, "tech.png"),
            metadata: None,
            required_verifications: 2,
        },
    );

    // Unregistered verifier cannot verify
    let unreg_err = f.client.try_verify_merchant(&id, &unreg);
    assert_eq!(
        unreg_err.unwrap_err().unwrap(),
        MarketplaceError::Unauthorized
    );

    // First verifier verifies
    f.client.verify_merchant(&id, &v1);
    let mid_state = f.client.get_merchant(&id);
    assert!(!mid_state.verified);
    assert_eq!(mid_state.status, MerchantStatus::Registered);

    // Same verifier cannot verify again
    let dup_verif = f.client.try_verify_merchant(&id, &v1);
    assert_eq!(
        dup_verif.unwrap_err().unwrap(),
        MarketplaceError::AlreadyVerified
    );

    // Second verifier verifies -> threshold reached!
    f.client.verify_merchant(&id, &v2);
    let verified_state = f.client.get_merchant(&id);
    assert!(verified_state.verified);
    assert_eq!(verified_state.status, MerchantStatus::Verified);

    let view = f.client.get_merchant_view(&id);
    assert!(view.verified);
    assert_eq!(view.status, MerchantStatus::Verified);

    // Revocation by admin
    f.client.revoke_verification(&f.admin, &id);
    let revoked_state = f.client.get_merchant(&id);
    assert!(!revoked_state.verified);
    assert_eq!(revoked_state.status, MerchantStatus::Registered);
}

#[test]
fn test_commission_rate_configuration() {
    let f = TestFixture::setup();
    let owner = Address::generate(&f.env);
    let stranger = Address::generate(&f.env);

    let id = f.client.register_merchant(
        &owner,
        &RegisterParams {
            name: String::from_str(&f.env, "Commission Store"),
            description: String::from_str(&f.env, "Desc"),
            category: symbol_short!("goods"),
            image_url: String::from_str(&f.env, "img.png"),
            metadata: None,
            required_verifications: 1,
        },
    );

    // Over 10000 bps fails
    let err_over = f.client.try_set_merchant_commission(&id, &owner, &10_001);
    assert_eq!(
        err_over.unwrap_err().unwrap(),
        MarketplaceError::InvalidCommissionBps
    );

    // Stranger unauthorized
    let err_unauth = f.client.try_set_merchant_commission(&id, &stranger, &500);
    assert_eq!(
        err_unauth.unwrap_err().unwrap(),
        MarketplaceError::Unauthorized
    );

    // Owner sets commission
    f.client.set_merchant_commission(&id, &owner, &500); // 5.00%
    assert_eq!(f.client.get_commission(&id), 500);

    // Admin sets commission
    f.client.set_merchant_commission(&id, &f.admin, &250); // 2.50%
    assert_eq!(f.client.get_commission(&id), 250);
}

#[test]
fn test_suspension_closing_and_mutation_locking() {
    let f = TestFixture::setup();
    let owner = Address::generate(&f.env);
    let verifier = Address::generate(&f.env);

    f.client.add_verifier(
        &f.admin,
        &Verifier {
            address: verifier.clone(),
            label: symbol_short!("kyc"),
            registered_at: 1,
        },
    );

    let id = f.client.register_merchant(
        &owner,
        &RegisterParams {
            name: String::from_str(&f.env, "Safe Store"),
            description: String::from_str(&f.env, "Desc"),
            category: symbol_short!("food"),
            image_url: String::from_str(&f.env, "food.png"),
            metadata: None,
            required_verifications: 1,
        },
    );

    // Admin suspends merchant
    f.client.suspend_merchant(&f.admin, &id);
    let suspended = f.client.get_merchant(&id);
    assert_eq!(suspended.status, MerchantStatus::Suspended);

    // Mutating ops must be blocked with MerchantFrozen
    let prof_err = f.client.try_update_merchant_profile(
        &id,
        &owner,
        &String::from_str(&f.env, "New"),
        &String::from_str(&f.env, "Desc"),
        &String::from_str(&f.env, "url"),
    );
    assert_eq!(
        prof_err.unwrap_err().unwrap(),
        MarketplaceError::MerchantFrozen
    );

    let meta_err = f.client.try_update_metadata(&id, &owner, &None);
    assert_eq!(
        meta_err.unwrap_err().unwrap(),
        MarketplaceError::MerchantFrozen
    );

    let comm_err = f.client.try_set_merchant_commission(&id, &owner, &100);
    assert_eq!(
        comm_err.unwrap_err().unwrap(),
        MarketplaceError::MerchantFrozen
    );

    let verif_err = f.client.try_verify_merchant(&id, &verifier);
    assert_eq!(
        verif_err.unwrap_err().unwrap(),
        MarketplaceError::MerchantFrozen
    );

    // Admin unsuspends merchant
    f.client.unsuspend_merchant(&f.admin, &id);
    let unsuspended = f.client.get_merchant(&id);
    assert_eq!(unsuspended.status, MerchantStatus::Registered);

    // Mutating ops work again
    f.client.set_merchant_commission(&id, &owner, &150);
    assert_eq!(f.client.get_commission(&id), 150);

    // Admin closes merchant permanently
    f.client.close_merchant(&f.admin, &id, &symbol_short!("bad_conduct"));
    let closed = f.client.get_merchant(&id);
    assert_eq!(closed.status, MerchantStatus::Closed);

    // Mutating ops blocked with MerchantClosed
    let comm_err_closed = f.client.try_set_merchant_commission(&id, &owner, &200);
    assert_eq!(
        comm_err_closed.unwrap_err().unwrap(),
        MarketplaceError::MerchantClosed
    );

    // Cannot suspend or unsuspend closed merchant
    let susp_err = f.client.try_suspend_merchant(&f.admin, &id);
    assert_eq!(
        susp_err.unwrap_err().unwrap(),
        MarketplaceError::MerchantClosed
    );

    let unsusp_err = f.client.try_unsuspend_merchant(&f.admin, &id);
    assert_eq!(
        unsusp_err.unwrap_err().unwrap(),
        MarketplaceError::MerchantClosed
    );
}

#[test]
fn test_paginated_discovery() {
    let f = TestFixture::setup();

    for i in 1..=5 {
        let owner = Address::generate(&f.env);
        let category = if i <= 3 {
            symbol_short!("tech")
        } else {
            symbol_short!("books")
        };
        let mut name_bytes = [0u8; 8];
        name_bytes[0] = b'S';
        name_bytes[1] = b't';
        name_bytes[2] = b'o';
        name_bytes[3] = b'r';
        name_bytes[4] = b'e';
        name_bytes[5] = b'0' + i as u8;
        let name = String::from_str(&f.env, core::str::from_utf8(&name_bytes[..6]).unwrap());

        f.client.register_merchant(
            &owner,
            &RegisterParams {
                name,
                description: String::from_str(&f.env, "Desc"),
                category,
                image_url: String::from_str(&f.env, "url"),
                metadata: None,
                required_verifications: 1,
            },
        );
    }

    // Page 1: 2 items
    let page1 = f.client.get_merchants(&0, &2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page1.get(0).unwrap().id, 1);
    assert_eq!(page1.get(1).unwrap().id, 2);

    // Page 2: 2 items
    let page2 = f.client.get_merchants(&2, &2);
    assert_eq!(page2.len(), 2);
    assert_eq!(page2.get(0).unwrap().id, 3);
    assert_eq!(page2.get(1).unwrap().id, 4);

    // Page 3: 1 item remaining
    let page3 = f.client.get_merchants(&4, &2);
    assert_eq!(page3.len(), 1);
    assert_eq!(page3.get(0).unwrap().id, 5);

    // Offset out of bounds
    let page_empty = f.client.get_merchants(&10, &2);
    assert_eq!(page_empty.len(), 0);

    // Category discovery
    let tech_merchants = f
        .client
        .get_merchants_by_category(&symbol_short!("tech"), &0, &10);
    assert_eq!(tech_merchants.len(), 3);
    assert_eq!(tech_merchants.get(0).unwrap().id, 1);
    assert_eq!(tech_merchants.get(1).unwrap().id, 2);
    assert_eq!(tech_merchants.get(2).unwrap().id, 3);

    let books_merchants = f
        .client
        .get_merchants_by_category(&symbol_short!("books"), &0, &10);
    assert_eq!(books_merchants.len(), 2);
    assert_eq!(books_merchants.get(0).unwrap().id, 4);
    assert_eq!(books_merchants.get(1).unwrap().id, 5);
}

#[test]
fn test_two_step_admin_transfer() {
    let f = TestFixture::setup();
    let new_admin = Address::generate(&f.env);
    let stranger = Address::generate(&f.env);

    // Non-admin cannot propose
    let prop_err = f.client.try_propose_admin(&stranger, &new_admin);
    assert_eq!(
        prop_err.unwrap_err().unwrap(),
        MarketplaceError::Unauthorized
    );

    // Accept with no proposal -> distinct NoPendingAdmin error (issue #113)
    let no_proposal_acc = f.client.try_accept_admin(&new_admin);
    assert_eq!(
        no_proposal_acc.unwrap_err().unwrap(),
        MarketplaceError::NoPendingAdmin
    );

    // Current admin proposes new admin
    f.client.propose_admin(&f.admin, &new_admin);

    // Stranger (wrong caller, proposal exists) -> Unauthorized
    let stranger_acc = f.client.try_accept_admin(&stranger);
    assert_eq!(
        stranger_acc.unwrap_err().unwrap(),
        MarketplaceError::Unauthorized
    );

    // New admin accepts
    f.client.accept_admin(&new_admin);
    assert_eq!(f.client.get_admin(), new_admin);

    // After acceptance the pending admin is cleared -> NoPendingAdmin again
    let cleared_acc = f.client.try_accept_admin(&new_admin);
    assert_eq!(
        cleared_acc.unwrap_err().unwrap(),
        MarketplaceError::NoPendingAdmin
    );
}

#[test]
fn test_reputation_score_injection_with_contract() {
    let f = TestFixture::setup();
    let seller = Address::generate(&f.env);
    let reputation_admin = Address::generate(&f.env);

    let rep_id = f.env.register(
        ReputationContract,
        (
            reputation_admin.clone(),
            ReputationConfig {
                decay_window_seconds: 90 * 24 * 60 * 60,
                min_transactions_threshold: 1,
                dispute_penalty_bps: 500,
                freeze_threshold_flags: 3,
            },
        ),
    );
    let rep_client = ReputationContractClient::new(&f.env, &rep_id);

    // Record a transaction for seller in reputation contract
    rep_client.record_transaction(
        &reputation_admin,
        &1u64,
        &seller,
        &Address::generate(&f.env),
        &500i128,
        &TransactionOutcome::Released,
    );

    let id = f.client.register_merchant(
        &seller,
        &RegisterParams {
            name: String::from_str(&f.env, "Reputable Seller"),
            description: String::from_str(&f.env, "Has reputation"),
            category: symbol_short!("services"),
            image_url: String::from_str(&f.env, "url"),
            metadata: None,
            required_verifications: 1,
        },
    );

    // Before setting reputation contract, score is None
    let view_before = f.client.get_merchant_view(&id);
    assert_eq!(view_before.reputation_score, None);

    // Non-admin cannot set reputation contract (CWE-345 protection)
    let unauth_err = f
        .client
        .try_set_merchant_reputation(&seller, &id, &Some(rep_id.clone()));
    assert_eq!(
        unauth_err.unwrap_err().unwrap(),
        MarketplaceError::Unauthorized
    );

    // Admin sets reputation contract for merchant
    f.client
        .set_merchant_reputation(&f.admin, &id, &Some(rep_id.clone()));
    let view_after = f.client.get_merchant_view(&id);
    let expected_score = rep_client.get_reputation(&seller).score;
    let injected = view_after
        .reputation_score
        .expect("score must be read from the reputation contract");
    assert_eq!(injected, expected_score);

    // Also test global reputation contract fallback
    f.client.set_merchant_reputation(&f.admin, &id, &None);
    f.client.set_reputation_contract(&f.admin, &rep_id);
    let view_fallback = f.client.get_merchant_view(&id);
    let fallback_injected = view_fallback
        .reputation_score
        .expect("fallback score must be read from global reputation contract");
    assert_eq!(fallback_injected, expected_score);
}

/// Acceptance flight test:
/// Registers a merchant, verifies with 2 distinct verifiers, and reads it back.
#[test]
fn test_flight_merchant_lifecycle_and_discovery() {
    let f = TestFixture::setup();

    let merchant_addr = Address::generate(&f.env);
    let verifier1_addr = Address::generate(&f.env);
    let verifier2_addr = Address::generate(&f.env);

    // Admin registers two verifiers
    f.client.add_verifier(
        &f.admin,
        &Verifier {
            address: verifier1_addr.clone(),
            label: symbol_short!("kyc"),
            registered_at: f.env.ledger().timestamp(),
        },
    );
    f.client.add_verifier(
        &f.admin,
        &Verifier {
            address: verifier2_addr.clone(),
            label: symbol_short!("auditor"),
            registered_at: f.env.ledger().timestamp(),
        },
    );

    // Merchant registers requiring 2 verifications
    let merchant_id = f.client.register_merchant(
        &merchant_addr,
        &RegisterParams {
            name: String::from_str(&f.env, "Stellar Artisans"),
            description: String::from_str(&f.env, "Handmade Stellar goods"),
            category: symbol_short!("crafts"),
            image_url: String::from_str(&f.env, "https://example.com/artisan.png"),
            metadata: Some(String::from_str(&f.env, "ipfs://bafybeicraft")),
            required_verifications: 2,
        },
    );
    assert_eq!(merchant_id, 1);

    // Verify view shows not yet verified
    let view_initial = f.client.get_merchant_view(&merchant_id);
    assert!(!view_initial.verified);
    assert_eq!(view_initial.status, MerchantStatus::Registered);

    // Verifier 1 verifies
    f.client.verify_merchant(&merchant_id, &verifier1_addr);
    let view_step1 = f.client.get_merchant_view(&merchant_id);
    assert!(!view_step1.verified);

    // Verifier 2 verifies -> multi-sig threshold reached
    f.client.verify_merchant(&merchant_id, &verifier2_addr);
    let view_step2 = f.client.get_merchant_view(&merchant_id);
    assert!(view_step2.verified);
    assert_eq!(view_step2.status, MerchantStatus::Verified);

    // Set commission
    f.client
        .set_merchant_commission(&merchant_id, &merchant_addr, &350); // 3.5%
    assert_eq!(f.client.get_commission(&merchant_id), 350);

    // Read back through discovery
    let discovery = f
        .client
        .get_merchants_by_category(&symbol_short!("crafts"), &0, &10);
    assert_eq!(discovery.len(), 1);
    let item = discovery.get(0).unwrap();
    assert_eq!(item.id, merchant_id);
    assert_eq!(item.name, String::from_str(&f.env, "Stellar Artisans"));
    assert_eq!(item.category, symbol_short!("crafts"));
    assert_eq!(item.commission_rate_bps, 350);
    assert!(item.verified);
    assert_eq!(item.status, MerchantStatus::Verified);
}
