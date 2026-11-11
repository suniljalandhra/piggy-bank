# SOL Vault: learning Pinocchio

A personal on-chain piggy bank. You deposit SOL into a PDA that only this program
controls, and only you can withdraw it.

Built with **Pinocchio 0.12**, **pinocchio-system 0.7**, tested with **Mollusk 0.17**.

## Quick start

```bash
cargo build-sbf     # compile to target/deploy/sol_vault.so (about 7.7 KB)
cargo test          # run the 10 Mollusk tests against that .so
```

Always run `cargo build-sbf` before `cargo test`. The tests load the compiled `.so`,
not your source code, so if you skip it you are testing a stale build.

## Suggested reading order

| # | File | What you learn |
|---|------|----------------|
| 1 | `src/lib.rs` | entrypoint, `no_std`, discriminator dispatch |
| 2 | `src/instructions/mod.rs` | hand-parsing a `u64`, PDA check, owner check |
| 3 | `src/instructions/deposit.rs` | account destructuring, signer check, plain `invoke` |
| 4 | `src/instructions/withdraw.rs` | `invoke_signed` with PDA seeds + bump |
| 5 | `tests/vault.rs` | building raw instructions, Mollusk, attack tests |

## Concept map: what Anchor did for you, and where it lives now

| Anchor | Pinocchio (this repo) |
|---|---|
| `#[program]` routes instructions | `match` on byte 0 in `process_instruction` |
| Borsh deserializes args | `u64::from_le_bytes(data.try_into()?)` |
| `Signer<'info>` | `if !user.is_signer() { return Err(...) }` |
| `seeds = [...], bump` | `Address::find_program_address` + compare |
| `Program<'info, System>` | `account.address() == &pinocchio_system::ID` |
| `CpiContext::new_with_signer` | `Transfer { .. }.invoke_signed(&[signer])` |

## Instruction format

```text
[ discriminator: u8 ][ amount: u64 little-endian ]
  0 = Deposit
  1 = Withdraw

accounts: [ user (signer, writable), vault PDA (writable), system program ]
```

## Pinocchio 0.12 naming (most tutorials online are older)

| Old (≤ 0.9) | 0.12 |
|---|---|
| `AccountInfo` | `AccountView` |
| `Pubkey` | `Address` |
| `account.key()` | `account.address()` |
| `pinocchio::program_error::ProgramError` | `pinocchio::error::ProgramError` |
| `pinocchio::instruction::{Seed, Signer}` | `pinocchio::cpi::{Seed, Signer}` |
| `accounts: &[AccountInfo]` | `accounts: &mut [AccountView]` |

## Things that will bite you

1. **Rent.** A 0-byte account must hold either 0 lamports or at least 890,880
   (rent-exempt). So your first deposit must be at least that much, and a withdraw
   must leave either 0 or at least that much. The runtime enforces this, not your
   program. See the `cannot_leave_dust_below_rent_exemption` test.
2. **Account order is part of your API.** Clients must send accounts in exactly
   the order your destructuring pattern expects.
3. **Every check you skip is a vulnerability.** Each test in the "security" section
   corresponds to one `if` in the program. Try deleting a check and watching which
   test fails. That is the fastest way to build intuition.

## Exercises (do them in order)

1. **Break it on purpose.** Comment out `check_vault_pda` in `withdraw.rs`, rebuild,
   and run the tests. Which test catches it? Put it back.
2. **Withdraw all.** Add instruction `2 = WithdrawAll` that takes no amount and
   sends `vault.lamports()` back to the user.
3. **Vault state account.** Store the bump in a program-owned account at
   `["state", user]` (create it with `pinocchio_system::instructions::CreateAccount`
   + `invoke_signed`). On withdraw, read the bump from it and use the cheaper
   `Address::derive_address` instead of `find_program_address`. Compare the
   compute units printed by the tests.
4. **Time lock.** Add an `unlock_at: i64` to the state account. On withdraw, read
   `Clock::get()?.unix_timestamp` (from `pinocchio::sysvars::clock`) and reject
   early withdrawals. In tests, use `mollusk.warp_to_slot(..)` or set
   `mollusk.sysvars.clock`.
5. **Close.** Add a `Close` instruction: send all vault lamports and all state-account
   lamports back to the user, then `state.close()` to reclaim the rent.
6. **Deploy to devnet.**
   ```bash
   solana config set --url devnet
   solana airdrop 2
   solana program deploy target/deploy/sol_vault.so
   ```
