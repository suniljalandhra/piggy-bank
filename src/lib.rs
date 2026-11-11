//! # SOL Vault: a personal on-chain piggy bank, written with Pinocchio
//!
//! Start reading here, then go to `instructions/deposit.rs`, then `instructions/withdraw.rs`.
//!
//! ## The big idea
//! Every user gets a vault at a PDA derived from `["vault", user_pubkey]`.
//! A PDA has no private key, so nobody can sign for it except this program
//! (through `invoke_signed`). The program's checks are the only gatekeeper.
//!
//! ## Instruction data layout (we parse it by hand, no Borsh)
//! ```text
//! byte 0      : discriminator  (0 = Deposit, 1 = Withdraw)
//! bytes 1..9  : amount in lamports, u64 little-endian
//! ```

// `no_std`: on-chain programs have no OS, so we don't use Rust's standard library.
// Pinocchio is built for this. (Tests still compile with std because they run on your laptop.)
#![cfg_attr(not(test), no_std)]

use pinocchio::{address::declare_id, error::ProgramError, AccountView, Address, ProgramResult};

pub mod instructions;
use instructions::{deposit, withdraw};

// The program's on-chain address. `cargo build-sbf` generated a keypair in
// target/deploy/sol_vault-keypair.json; this is its public key.
// After a fresh clone, run `solana address -k target/deploy/sol_vault-keypair.json`
// and paste the result here.
declare_id!("2X32q6SPzwu349onbTjGeECPNVD5E5g9bchzwPgamqat");

/// Seed prefix for every vault PDA. Clients must use exactly the same bytes.
pub const VAULT_SEED: &[u8] = b"vault";

// ---------------------------------------------------------------------------
// Entrypoint
// ---------------------------------------------------------------------------
// The runtime hands the program one raw byte buffer. `program_entrypoint!` parses
// that buffer into (program_id, accounts, instruction_data) with zero copying, then
// calls our function.
//
// Pinocchio's all-in-one `entrypoint!` macro also sets up a heap allocator and a
// std-style panic hook. We're `no_std` and never allocate, so we wire up the three
// pieces ourselves instead:
//   * `program_entrypoint!`  - the actual `entrypoint` symbol the runtime calls
//   * `no_allocator!`        - any heap allocation aborts (we don't need a heap)
//   * `nostd_panic_handler!` - logs file:line on panic, then aborts
//
// Only compile these for the real on-chain build. When the integration tests link
// this crate as a normal library, there is no `entrypoint` symbol to export.
#[cfg(target_os = "solana")]
mod entrypoint {
    pinocchio::program_entrypoint!(crate::process_instruction);
    pinocchio::no_allocator!();
    pinocchio::nostd_panic_handler!();
}

/// Reads the first byte (the discriminator) and routes to the matching handler.
/// This is the job Anchor's `#[program]` macro normally does for you.
pub fn process_instruction(
    program_id: &Address,
    accounts: &mut [AccountView],
    instruction_data: &[u8],
) -> ProgramResult {
    // `split_first` gives us (first_byte, rest) or None if the data is empty.
    let (discriminator, data) = instruction_data
        .split_first()
        .ok_or(ProgramError::InvalidInstructionData)?;

    match *discriminator {
        deposit::DISCRIMINATOR => deposit::process(program_id, accounts, data),
        withdraw::DISCRIMINATOR => withdraw::process(program_id, accounts, data),
        _ => Err(ProgramError::InvalidInstructionData),
    }
}
