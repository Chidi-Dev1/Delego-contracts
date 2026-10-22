use soroban_sdk::{contracttype, Symbol};

/// Pause metadata stored alongside the `Paused` lifecycle state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PauseInfo {
    /// Ledger sequence at which the contract was paused.
    pub paused_at_ledger: u32,
    /// Short symbolic reason for the pause.
    pub reason: Symbol,
}

/// Contract-wide lifecycle state shared by emergency-pausable contracts.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractLifecycleState {
    /// Normal operations are enabled.
    Active,
    /// Emergency pause — carries audit metadata in a `PauseInfo` struct.
    Paused(PauseInfo),
}
