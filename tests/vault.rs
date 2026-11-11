//! Integration tests: load the compiled `target/deploy/sol_vault.so` into Mollusk
//! (an in-process Solana VM) and fire real instructions at it.
//!
//! Run with:  cargo build-sbf && cargo test

use mollusk_svm::{program::keyed_account_for_system_program, result::ProgramResult, Mollusk};
use pinocchio::error::ProgramError;
use solana_account::Account;
use solana_instruction::{AccountMeta, Instruction};
use solana_pubkey::Pubkey;

const LAMPORTS_PER_SOL: u64 = 1_000_000_000;

// ---------- helpers ---------------------------------------------------------

fn program_id() -> Pubkey {
    Pubkey::new_from_array(*sol_vault::ID.as_array())
}

fn setup() -> Mollusk {
    // Looks for `target/deploy/sol_vault.so`
    Mollusk::new(&program_id(), "sol_vault")
}

fn vault_pda(user: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[sol_vault::VAULT_SEED, user.as_ref()], &program_id()).0
}

/// Build instruction data by hand: [discriminator][amount as u64 LE]
fn ix_data(discriminator: u8, amount: u64) -> Vec<u8> {
    let mut data = vec![discriminator];
    data.extend_from_slice(&amount.to_le_bytes());
    data
}

fn ix(discriminator: u8, user: Pubkey, vault: Pubkey, amount: u64, user_signs: bool) -> Instruction {
    Instruction::new_with_bytes(
        program_id(),
        &ix_data(discriminator, amount),
        vec![
            AccountMeta::new(user, user_signs),
            AccountMeta::new(vault, false),
            AccountMeta::new_readonly(solana_pubkey::pubkey!("11111111111111111111111111111111"), false),
        ],
    )
}

fn deposit_ix(user: Pubkey, vault: Pubkey, amount: u64) -> Instruction {
    ix(sol_vault::instructions::deposit::DISCRIMINATOR, user, vault, amount, true)
}

fn withdraw_ix(user: Pubkey, vault: Pubkey, amount: u64) -> Instruction {
    ix(sol_vault::instructions::withdraw::DISCRIMINATOR, user, vault, amount, true)
}

fn wallet(lamports: u64) -> Account {
    Account::empty(lamports, &Pubkey::default()) // no data, owned by the System Program (111...1)
}

/// Accounts for a user who already has `vault_balance` lamports in their vault.
fn accounts(user: Pubkey, user_balance: u64, vault_balance: u64) -> Vec<(Pubkey, Account)> {
    vec![
        (user, wallet(user_balance)),
        (vault_pda(&user), wallet(vault_balance)),
        keyed_account_for_system_program(),
    ]
}

fn lamports_of(result: &mollusk_svm::result::InstructionResult, key: &Pubkey) -> u64 {
    result.get_account(key).unwrap().lamports
}

// ---------- happy paths -----------------------------------------------------

#[test]
fn deposit_moves_sol_into_vault() {
    let mollusk = setup();
    let user = Pubkey::new_unique();
    let vault = vault_pda(&user);

    let result = mollusk.process_instruction(
        &deposit_ix(user, vault, LAMPORTS_PER_SOL),
        &accounts(user, 10 * LAMPORTS_PER_SOL, 0),
    );

    assert_eq!(result.program_result, ProgramResult::Success);
    assert_eq!(lamports_of(&result, &user), 9 * LAMPORTS_PER_SOL);
    assert_eq!(lamports_of(&result, &vault), LAMPORTS_PER_SOL);
    println!("deposit used {} compute units", result.compute_units_consumed);
}

#[test]
fn deposit_then_withdraw_everything() {
    let mollusk = setup();
    let user = Pubkey::new_unique();
    let vault = vault_pda(&user);

    // A chain runs the instructions back to back, carrying account state forward.
    let result = mollusk.process_instruction_chain(
        &[
            deposit_ix(user, vault, 2 * LAMPORTS_PER_SOL),
            withdraw_ix(user, vault, 2 * LAMPORTS_PER_SOL),
        ],
        &accounts(user, 10 * LAMPORTS_PER_SOL, 0),
    );

    assert_eq!(result.program_result, ProgramResult::Success);
    assert_eq!(lamports_of(&result, &user), 10 * LAMPORTS_PER_SOL);
    assert_eq!(lamports_of(&result, &vault), 0);
}

#[test]
fn partial_withdraw_that_stays_rent_exempt() {
    let mollusk = setup();
    let user = Pubkey::new_unique();
    let vault = vault_pda(&user);

    let result = mollusk.process_instruction(
        &withdraw_ix(user, vault, LAMPORTS_PER_SOL),
        &accounts(user, LAMPORTS_PER_SOL, 3 * LAMPORTS_PER_SOL),
    );

    assert_eq!(result.program_result, ProgramResult::Success);
    assert_eq!(lamports_of(&result, &vault), 2 * LAMPORTS_PER_SOL);
    println!("withdraw used {} compute units", result.compute_units_consumed);
}

// ---------- security: each check in the program has a test that trips it ----

#[test]
fn attacker_cannot_withdraw_from_someone_elses_vault() {
    let mollusk = setup();
    let victim = Pubkey::new_unique();
    let attacker = Pubkey::new_unique();
    let victims_vault = vault_pda(&victim);

    // The attacker signs as themselves but points at the victim's vault.
    let result = mollusk.process_instruction(
        &withdraw_ix(attacker, victims_vault, LAMPORTS_PER_SOL),
        &[
            (attacker, wallet(LAMPORTS_PER_SOL)),
            (victims_vault, wallet(5 * LAMPORTS_PER_SOL)),
            keyed_account_for_system_program(),
        ],
    );

    // PDA("vault", attacker) != victims_vault, so the program rejects it.
    assert_eq!(result.program_result, ProgramResult::Failure(ProgramError::InvalidSeeds));
}

#[test]
fn withdraw_requires_the_owner_signature() {
    let mollusk = setup();
    let user = Pubkey::new_unique();
    let vault = vault_pda(&user);

    // Correct accounts, but `user` did not sign.
    let unsigned = ix(sol_vault::instructions::withdraw::DISCRIMINATOR, user, vault, LAMPORTS_PER_SOL, false);
    let result = mollusk.process_instruction(&unsigned, &accounts(user, 0, 5 * LAMPORTS_PER_SOL));

    assert_eq!(
        result.program_result,
        ProgramResult::Failure(ProgramError::MissingRequiredSignature)
    );
}

#[test]
fn cannot_withdraw_more_than_the_vault_holds() {
    let mollusk = setup();
    let user = Pubkey::new_unique();
    let vault = vault_pda(&user);

    let result = mollusk.process_instruction(
        &withdraw_ix(user, vault, 2 * LAMPORTS_PER_SOL),
        &accounts(user, 0, LAMPORTS_PER_SOL),
    );

    assert_eq!(result.program_result, ProgramResult::Failure(ProgramError::InsufficientFunds));
}

/// Our program doesn't check rent at all: the *runtime* does, after the
/// instruction finishes. Mollusk mirrors that with a post-execution check that
/// panics, which is why this test expects a panic instead of a ProgramError.
#[test]
#[should_panic(expected = "insufficient funds for rent")]
fn cannot_leave_dust_below_rent_exemption() {
    let mollusk = setup();
    let user = Pubkey::new_unique();
    let vault = vault_pda(&user);

    // Leave 1 lamport behind: not 0, and not rent-exempt, so the runtime refuses.
    mollusk.process_instruction(
        &withdraw_ix(user, vault, LAMPORTS_PER_SOL - 1),
        &accounts(user, 0, LAMPORTS_PER_SOL),
    );
}

#[test]
fn rejects_zero_amount() {
    let mollusk = setup();
    let user = Pubkey::new_unique();
    let vault = vault_pda(&user);

    let result =
        mollusk.process_instruction(&deposit_ix(user, vault, 0), &accounts(user, LAMPORTS_PER_SOL, 0));

    assert_eq!(result.program_result, ProgramResult::Failure(ProgramError::InvalidInstructionData));
}

#[test]
fn rejects_unknown_discriminator_and_short_data() {
    let mollusk = setup();
    let user = Pubkey::new_unique();
    let vault = vault_pda(&user);
    let accts = accounts(user, LAMPORTS_PER_SOL, 0);

    let mut bad = deposit_ix(user, vault, 1);
    bad.data[0] = 99; // no such instruction
    let r = mollusk.process_instruction(&bad, &accts);
    assert_eq!(r.program_result, ProgramResult::Failure(ProgramError::InvalidInstructionData));

    let mut short = deposit_ix(user, vault, 1);
    short.data.truncate(5); // amount needs 8 bytes, we give 4
    let r = mollusk.process_instruction(&short, &accts);
    assert_eq!(r.program_result, ProgramResult::Failure(ProgramError::InvalidInstructionData));
}

#[test]
fn rejects_a_fake_system_program() {
    let mollusk = setup();
    let user = Pubkey::new_unique();
    let vault = vault_pda(&user);
    let fake = Pubkey::new_unique();

    let mut instruction = deposit_ix(user, vault, LAMPORTS_PER_SOL);
    instruction.accounts[2] = AccountMeta::new_readonly(fake, false);

    let result = mollusk.process_instruction(
        &instruction,
        &[
            (user, wallet(10 * LAMPORTS_PER_SOL)),
            (vault, wallet(0)),
            (fake, wallet(0)),
        ],
    );

    assert_eq!(result.program_result, ProgramResult::Failure(ProgramError::IncorrectProgramId));
}
