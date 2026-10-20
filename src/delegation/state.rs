use soroban_sdk::{contracttype, Address, Env, Symbol, Vec};
use soroban_sdk::token::{Token, TokenType};
use soroban_sdk::types::ContractData;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
put enum AccountCircuitBreakerStatus {
    Normal,
    Frozen,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EpochConfig {
    pub current_epoch: u32,
    pub epoch_started_ledger: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DelegationState {
    pub owner: Address,
    pub agent: Address,
    pub is_paused: bool,
    pub epoch_config: EpochConfig,
    pub permissions: Vec<Permission>,
}

impl DelegationState {
    pub fn new(env: &Env, owner: Address, agent: Address) -> Self {
        DelegationState {
            owner,
            agent,
            is_paused: false,
            epoch_config: EpochConfig {
                current_epoch: 0,
                epoch_started_ledger: env.ledger().sequence(),
            },
            permissions: Vec::new(env),
        }
    }

    pub fn increment_epoch(&mut self, env: &Env) {
        self.epoch_config.current_epoch += 1;
        self.epoch_config.epoch_started_ledger = env.ledger().sequence();
    }
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AccountCircuitBreaker {
    pub delegator: Address,
    pub status: AccountCircuitBreakerStatus,
    pub frozen_epoch: u32,
    pub frozen_at: u32,
}
