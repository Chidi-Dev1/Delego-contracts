use soroban_sdk::{contracterror, Env};

#[contracterror]
#[derive(Debug, Eq, PartialEq)]
pub enum PermissionError {
    #[msg("Permission denied")]
    PermissionDenied,

    #[msg("Signature epoch is stale")]
    StaleEpoch,

    #[msg("Invalid authorization signature")]
    InvalidSignature,

    #[msg("Token asset is not authorized for this delegation")]
    AssetNotAuthorized,

    #[msg("Spend exceeds the configured allowance")]
    ExceedsAllowance,

    #[msg("Delegation is paused")]
    DelegationPaused,

    #[msg("Delegation has expired")]
    DelegationExpired,
}
