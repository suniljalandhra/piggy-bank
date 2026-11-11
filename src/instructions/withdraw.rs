//! # Withdraw: move SOL from the vault PDA back to its owner.
//!
//! Accounts:
//!   0. `[signer, writable]` user:           must be the vault's owner
//!   1. `[writable]`         vault:          PDA `["vault", user]`
//!   2. `[]`                 system_program: needed for the CPI
//!
//! Data: `amount: u64` (little-endian), the lamports to withdraw.
//!
//! ## Rent gotcha
//! An account must either hold at least the rent-exempt minimum
//! (890,880 lamports for a 0-byte account) or hold exactly 0.
//! So you can withdraw everything, or leave at least that minimum behind.
//! Anything in between fails with `InsufficientFundsForRent`.

use pinocchio::{
    cpi::{Seed, Signer},
    error::ProgramError,
    AccountView, Address, ProgramResult,
};
use pinocchio_system::instructions::Transfer;

use super::{check_system_owned, check_system_program, check_vault_pda, parse_amount};
use crate::VAULT_SEED;

pub const DISCRIMINATOR: u8 = 1;

pub fn process(program_id: &Address, accounts: &mut [AccountView], data: &[u8]) -> ProgramResult {
    let [user, vault, system_program, ..] = accounts else {
        return Err(ProgramError::NotEnoughAccountKeys);
    };

    let amount = parse_amount(data)?;

    // ---- Ownership check ---------------------------------------------------
    // The vault doesn't store who owns it. Instead, ownership is *implied by the address*:
    // vault == PDA("vault", user) proves this vault belongs to `user`.
    // And `user.is_signer()` proves the caller actually controls `user`.
    // Together: only the owner can withdraw from their own vault.
    if !user.is_signer() {
        return Err(ProgramError::MissingRequiredSignature);
    }
    let bump = check_vault_pda(vault, user, program_id)?;
    check_system_owned(vault)?;
    check_system_program(system_program)?;

    if vault.lamports() < amount {
        return Err(ProgramError::InsufficientFunds);
    }

    // ---- PDA-signed CPI ----------------------------------------------------
    // The System Program will only debit `vault` if `vault` signed. A PDA has no
    // private key, so we give the runtime the seeds + bump instead. The runtime
    // re-derives the address from them, checks it matches `vault`, and checks the
    // caller is the program the PDA belongs to. If both hold, it counts as a signature.
    let bump_bytes = [bump];
    let seeds = [
        Seed::from(VAULT_SEED),
        Seed::from(user.address().as_ref()),
        Seed::from(&bump_bytes),
    ];
    let signer = Signer::from(&seeds);

    Transfer {
        from: vault,
        to: user,
        lamports: amount,
    }
    .invoke_signed(&[signer])
}
