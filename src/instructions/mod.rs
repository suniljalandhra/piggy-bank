//! Shared helpers used by both instructions.

pub mod deposit;
pub mod withdraw;

use pinocchio::{error::ProgramError, AccountView, Address};

use crate::VAULT_SEED;

/// Parse the 8-byte little-endian `amount` that follows the discriminator.
///
/// Anchor would deserialize this with Borsh. Here we do it by hand:
/// take exactly 8 bytes, turn them into a `[u8; 8]`, and read a `u64`.
pub fn parse_amount(data: &[u8]) -> Result<u64, ProgramError> {
    // `try_into` fails unless the slice is exactly 8 bytes long.
    let bytes: [u8; 8] = data
        .try_into()
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let amount = u64::from_le_bytes(bytes);

    if amount == 0 {
        return Err(ProgramError::InvalidInstructionData);
    }
    Ok(amount)
}

/// Make sure `vault` is the PDA `["vault", user]` for this program, and return its bump.
///
/// Without this check, an attacker could pass *someone else's* vault, or any
/// random account, and the program would happily move funds around.
pub fn check_vault_pda(
    vault: &AccountView,
    user: &AccountView,
    program_id: &Address,
) -> Result<u8, ProgramError> {
    // `find_program_address` tries bumps 255, 254, ... until it gets an address
    // that is NOT on the ed25519 curve (so it has no private key). The first one
    // that works is the "canonical bump".
    let (expected, bump) =
        Address::find_program_address(&[VAULT_SEED, user.address().as_ref()], program_id);

    if vault.address() != &expected {
        return Err(ProgramError::InvalidSeeds);
    }
    Ok(bump)
}

/// The vault holds only lamports and no data, so it must be owned by the
/// System Program. That is also what lets the System Program move SOL out of it.
pub fn check_system_owned(account: &AccountView) -> Result<(), ProgramError> {
    if !account.owned_by(&pinocchio_system::ID) {
        return Err(ProgramError::InvalidAccountOwner);
    }
    Ok(())
}

/// Make sure the account passed as "system program" really is the System Program.
pub fn check_system_program(account: &AccountView) -> Result<(), ProgramError> {
    if account.address() != &pinocchio_system::ID {
        return Err(ProgramError::IncorrectProgramId);
    }
    Ok(())
}
