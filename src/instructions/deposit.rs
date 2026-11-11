//! # Deposit: move SOL from the user's wallet into their vault PDA.
//!
//! Accounts (order matters, the client must send them in this order):
//!   0. `[signer, writable]` user:           pays the SOL
//!   1. `[writable]`         vault:          PDA `["vault", user]`
//!   2. `[]`                 system_program: needed for the CPI
//!
//! Data: `amount: u64` (little-endian), the lamports to deposit.

use pinocchio::{error::ProgramError, AccountView, Address, ProgramResult};
use pinocchio_system::instructions::Transfer;

use super::{check_system_owned, check_system_program, check_vault_pda, parse_amount};

pub const DISCRIMINATOR: u8 = 0;

pub fn process(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    // 1. Unpack accounts. The slice pattern fails cleanly if too few were passed.
    let [user, vault, system_program, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    // 2. Parse instruction data.
    let amount = parse_amount(data)?;

    // 3. Validate accounts. This is the part Anchor's `#[derive(Accounts)]` does for you.
    if !user.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    check_vault_pda(vault, user, program_id)?; // bump not needed: the user signs, not the PDA
    check_system_owned(vault)?;
    check_system_program(system_program)?;

    // 4. CPI into the System Program.
    // The *user* is the `from` account and already signed the transaction, so
    // their signature carries through. A plain `invoke` is enough.
    Transfer {
        from: user,
        to: vault,
        lamports: amount,
    }
    .invoke()
}
