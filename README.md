# Independent Burn Bridge

An independent two-program CCTP-like Solana bridge. `deposit_for_burn` burns
through SPL Token and invokes the separate Message Transmitter's `send_message`
through CPI, producing a comparable explorer instruction tree. This is not
Circle CCTP and receives no Circle attestations.

## Instructions

- `initialize(local_domain, attester)`
- `deposit_for_burn(amount, destination_domain, mint_recipient)`
- `deposit_for_burn_with_caller(...)`
- `replace_deposit_for_burn(...)`
- `send_message(destination_domain, recipient, destination_caller, body)`
- `receive_message(source_domain, nonce, sender, recipient, body_hash)`
- `mint_and_withdraw(source_domain, nonce, amount)`
- `pause`, `unpause`, `update_attester`
- `transfer_ownership`, `accept_ownership`

## Mainnet deployment

### No-install deployment with GitHub Actions

1. Create a **private** GitHub repository and upload everything in this folder.
2. In repository Settings > Secrets and variables > Actions, add:
   - `SOLANA_PRIVATE_KEY`: the Base58 private key of a dedicated funded wallet.
   - `SOLANA_RPC_URL`: your mainnet RPC URL.
3. Open Actions > Deploy independent bridge to Solana mainnet > Run workflow.
4. Read both program IDs and deployment signatures in the job output or the
   `deployment-details` artifact.

No local development tools are required for this route.

### Local deployment

Install Solana CLI, Rust, and Anchor 0.30.1. Edit `RPC_URL` and
`PRIVATE_KEY_BASE58` at the top of `deploy.py`, then run
`python3 deploy.py` with no arguments or prompts.
It generates two program keypairs, rewrites both IDs, builds, deploys the Message
Transmitter followed by the Token Messenger/Minter, and prints both program IDs
and deployment signatures. Test on localnet/devnet and obtain an independent
audit before mainnet use.
