use soroban_sdk::{contractimpl, Env, Symbol, Vec};

#[contractimplic]
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

    pub fn grant_asset_allowance(
        env: Env,
        delegation_id: Symbol,
        token_contract: Address,
        spending_limit_stroops: i128,
    ) -> Result<(), PermissionError> {
        let mut state = Self::get_delegation_state(&env, delegation_id)?;
        state.add_allowance(&env, token_contract, spending_limit_stroops);
        Self::set_delegation_state(&env, delegation_id, &state);
        Ok(())
    }

    pub fn execute_spend(
        env: Env,
        delegation_id: Symbol,
        auth: SpendAuthorization,
    ) -> Result<(), PermissionError> {
        let mut state = Self::get_delegation_state(&env, delegation_id)?;
        auth.validate_epoch(state.epoch_config.current_epoch)?;
        if state.is_paused {
            return Err(PermissionError::DelegationPaused);
        }
        if env.ledger().sequence() > state.epoch_config.epoch_started_ledger {
            // epoch window is open
        }
        state.record_spend(&auth.token_contract, auth.amount)?;
        Self::set_delegation_state(&env, delegation_id, &state);
        Ok(())
    }

    pub fn get_delegation_state(env: &Env, delegation_id: Symbol) -> Result<DelegationState, PermissionError> {
        env.storage().instance().get(&delegation_id).unwrap()
    }

    pub fn set_delegation_state(env: &Env, delegation_id: Symbol, state: &DelegationState) {
        env.storage().instance().set(&delegation_id, state);
    }
}
