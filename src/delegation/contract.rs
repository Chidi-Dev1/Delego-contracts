use soroban_sdk::{contractimpl, contracttype, Env, Symbol, Vec};
use soroban_sdk::token::{Token, TokenType};
use soroban_sdk::types::ContractData;

#[contracttype]
pub enum AccountCircuitBreakerStatus {
    Normal,
    Frozen,
}

#[contractimpl]
pub struct DelegationContract;

#[contractimpl]
impl DelegationContract {
    pub fn pause_delegation(env: Env, delegation_id: Symbol) -> Result<(), PermissionError> {
        let mut state = Self::get_delegation_state(&env, delegation_id)?;
        state.increment_epoch(&env);
        state.is_paused = true;
        Self::set_delegation_state(&env, delegation_id, &state);
        Ok(())
    }

    pub fn resume_delegation(env: Env, delegation_id: Symbol) -> Result<(), PermissionError> {
        let mut state = Self::get_delegation_state(&env, delegation_id)?;
        state.increment_epoch(&env);
        state.is_paused = false;
        Self::set_delegation_state(&env, delegation_id, &state);
        Ok(())
    }

    pub fn trigger_emergency_killswitch(
        env: Env,
        delegator: Symbol,
    ) -> Result<(), PermissionError> {
        let mut state = Self::get_delegation_state(&env, delegator.clone())?;
        state.increment_epoch(&env);
        state.is_paused = true;
        Self::set_delegation_state(&env, delegator.clone(), &state);
        Self::set_circuit_breaker_status(
            &env,
            delegator.clone(),
            AccountCircuitBreakerStatus::Frozen,
        );
        env.events().publish(
            (Symbol::new(&env, "DelegationCircuitBreakerTriggered"), delegator),
            state.epoch_config.current_epoch,
        );
        Ok(())
    }

    pub fn reactivate_account(
        env: Env,
        delegator: Symbol,
    ) -> Result<(), PermissionError> {
        delegator.require_auth();
        let mut state = Self::get_delegation_state(&env, delegator.clone())?;
        state.increment_epoch(&env);
        state.is_paused = false;
        Self::set_delegation_state(&env, delegator.clone(), &state);
        Self::set_circuit_breaker_status(
            &env,
            delegator,
            AccountCircuitBreakerStatus::Normal,
        );
        Ok(())
    }

    pub fn execute_spend(
        env: Env,
        delegation_id: Symbol,
        auth: SpendAuthorization,
    ) -> Result<(), PermissionError> {
        if let AccountCircuitBreakerStatus::Frozen =
            Self::get_circuit_breaker_status(&env, delegation_id.clone())
        {
            return Err(PermissionError::Unauthorized);
        }
        let state = Self::get_delegation_state(&env, delegation_id)?;
        auth.validate_epoch(state.epoch_config.current_epoch)?;
        // ... existing spend logic
        Ok(())
    }
}