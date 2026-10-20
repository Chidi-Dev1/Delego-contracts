use soroban_sdk::{test::Ledger, Address, Env, Symbol};

#[test]
fn test_epoch_increment_on_pause_resume() {
    let env = Env::default();
    let client = DelegationContractClient::new(&env, &DelegationContract);

    let delegation_id = Symbol::from_utf8(&env, "test_delegation").unwrap();
    let owner = Address::generate(&env);
    let agent = Address::generate(&env);

    // Initial state
    let initial_state = client.get_delegation_state(&delegation_id);
    assert_eq!(initial_state.epoch_config.current_epoch, 0);

    // Pause increments epoch
    client.pause_delegation(&delegation_id);
    let paused_state = client.get_delegation_state(&delegation_id);
    assert_eq!(paused_state.epoch_config.current_epoch, 1);

    // Resume increments epoch again
    client.resume_delegation(&delegation_id);
    let resumed_state = client.get_delegation_state(&delegation_id);
    assert_eq!(resumed_state.epoch_config.current_epoch, 2);
}

#[test]
fn test_stale_signature_rejection() {
    let env = Env::default();
    let client = DelegationContractClient::new(&env, &DelegationContract);

    let delegation_id = Symbol::from_utf8(&env, "test_delegation").unwrap();
    let owner = Address::generate(&env);
    let agent = Address::generate(&env);
    let token = Address::generate(&env);

    // Create auth with epoch 0
    let auth = SpendAuthorization {
        delegator: owner,
        agent,
        token_contract: token,
        amount: 100,
        recipient: Address::generate(&env),
        nonce: 0,
        epoch: 0,
        signature: vec![0u8; 64],
    };

    // Pause/resume to advance epoch
    client.pause_delegation(&delegation_id);
    client.resume_delegation(&delegation_id);

    // Should fail with StaleEpoch
    let result = client.execute_spend(&delegation_id, auth);
    assert(matches(result, Err(PermissionError::StaleEpoch)));
}

#[test]
fn test_multi_asset_independent_limits() {
    let env = Env::default();
    let client = DelegationContractClient::new(&env, &DelegationContract);

    let delegation_id = Symbol::from_utf8(&env, "test_delegation").unwrap();
    let owner = Address::generate(&env);
    let agent = Address::generate(&env);
    let usdc = Address::generate(&env);
    let eurc = Address::generate(&env);

    client.grant_asset_allowance(&delegation_id, &usdc, &1000);
    client.grant_asset_allowance(&delegation_id, &eurc, &500);

    // Spend 400 USDC
    let auth_usdc = SpendAuthorization {
        delegator: owner.clone(),
        agent: agent.clone(),
        token_contract: usdc.clone(),
        amount: 400,
        recipient: Address::generate(&env),
        nonce: 0,
        epoch: 0,
        signature: vec![0u8; 64],
    };
    client.execute_spend(&delegation_id, auth_usdc);

    // Spend 500 EURC - should succeed independently
    let auth_eurc = SpendAuthorization {
        delegator: owner.clone(),
        agent: agent.clone(),
        token_contract: eurc.clone(),
        amount: 500,
        recipient: Address::generate(&env),
        nonce: 1,
        epoch: 0,
        signature: vec![0u8; 64],
    };
    client.execute_spend(&delegation_id, auth_eurc);

    // USDC limit still has 600 remaining; EURC is exhausted
    let auth_over_usdc = SpendAuthorization {
        delegator: owner.clone(),
        agent: agent.clone(),
        token_contract: usdc.clone(),
        amount: 700,
        recipient: Address::generate(&env),
        nonce: 2,
        epoch: 0,
        signature: vec![0u8; 64],
    };
    let result = client.execute_spend(&delegation_id, auth_over_usdc);
    assert(matches(result, Err(PermissionError::ExceedsAllowance)));
}

#[test]
fn test_unauthorized_asset_rejected() {
    let env = Env::default();
    let client = DelegationContractClient::new(&env, &DelegationContract);

    let delegation_id = Symbol::from_utf8(&env, "test_delegation").unwrap();
    let owner = Address::generate(&env);
    let agent = Address::generate(&env);
    let usdc = Address::generate(&env);
    let xlq = Address::generate(&env);

    client.grant_asset_allowance(&delegation_id, &usdc, &1000);

    let auth = SpendAuthorization {
        delegator: owner,
        agent,
        token_contract: xlq,
        amount: 100,
        recipient: Address::generate(&env),
        nonce: 0,
        epoch: 0,
        signature: vec![0u8; 64],
    };
    let result = client.execute_spend(&delegation_id, auth);
    assert(matches(result, Err(PermissionError::AssetNotAuthorized)));
}
