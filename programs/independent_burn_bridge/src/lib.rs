use anchor_lang::prelude::*;
use anchor_spl::token::{self, Burn, Mint, MintTo, Token, TokenAccount};
use message_transmitter::program::MessageTransmitter;

declare_id!("Brdg111111111111111111111111111111111111111");

#[program]
pub mod independent_burn_bridge {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, local_domain: u32, attester: Pubkey) -> Result<()> {
        let state = &mut ctx.accounts.state;
        state.admin = ctx.accounts.admin.key();
        state.pending_admin = Pubkey::default();
        state.attester = attester;
        state.local_domain = local_domain;
        state.next_nonce = 0;
        state.paused = false;
        state.bump = ctx.bumps.state;
        Ok(())
    }

    pub fn deposit_for_burn(
        ctx: Context<DepositForBurn>, amount: u64, destination_domain: u32,
        mint_recipient: [u8; 32],
    ) -> Result<()> {
        deposit(ctx, amount, destination_domain, mint_recipient, [0; 32])
    }

    pub fn deposit_for_burn_with_caller(
        ctx: Context<DepositForBurn>, amount: u64, destination_domain: u32,
        mint_recipient: [u8; 32], destination_caller: [u8; 32],
    ) -> Result<()> {
        require!(destination_caller != [0; 32], BridgeError::InvalidCaller);
        deposit(ctx, amount, destination_domain, mint_recipient, destination_caller)
    }

    pub fn replace_deposit_for_burn(
        ctx: Context<ReplaceMessage>, new_mint_recipient: [u8; 32],
        new_destination_caller: [u8; 32],
    ) -> Result<()> {
        let message = &mut ctx.accounts.message;
        require!(!message.finalized, BridgeError::AlreadyFinalized);
        message.mint_recipient = new_mint_recipient;
        message.destination_caller = new_destination_caller;
        message.replacement_count = message.replacement_count.checked_add(1).ok_or(BridgeError::Overflow)?;
        emit!(MessageReplaced { nonce: message.nonce, sender: message.sender,
            mint_recipient: new_mint_recipient, destination_caller: new_destination_caller });
        Ok(())
    }

    pub fn send_message(
        ctx: Context<SendMessage>, destination_domain: u32, recipient: [u8; 32],
        destination_caller: [u8; 32], body: Vec<u8>,
    ) -> Result<()> {
        require!(!ctx.accounts.state.paused, BridgeError::Paused);
        require!(body.len() <= 1024, BridgeError::MessageTooLarge);
        let nonce = ctx.accounts.state.next_nonce;
        ctx.accounts.state.next_nonce = nonce.checked_add(1).ok_or(BridgeError::Overflow)?;
        let message = &mut ctx.accounts.message;
        message.nonce = nonce;
        message.source_domain = ctx.accounts.state.local_domain;
        message.destination_domain = destination_domain;
        message.sender = ctx.accounts.sender.key();
        message.recipient = recipient;
        message.destination_caller = destination_caller;
        message.body = body.clone();
        message.finalized = false;
        message.bump = ctx.bumps.message;
        emit!(MessageSent { nonce, source_domain: message.source_domain, destination_domain,
            sender: message.sender, recipient, destination_caller, body });
        Ok(())
    }

    pub fn receive_message(
        ctx: Context<ReceiveMessage>, source_domain: u32, nonce: u64,
        sender: [u8; 32], recipient: Pubkey, body_hash: [u8; 32],
    ) -> Result<()> {
        require!(!ctx.accounts.state.paused, BridgeError::Paused);
        require_keys_eq!(ctx.accounts.attester.key(), ctx.accounts.state.attester, BridgeError::UnauthorizedAttester);
        let receipt = &mut ctx.accounts.receipt;
        receipt.source_domain = source_domain;
        receipt.nonce = nonce;
        receipt.sender = sender;
        receipt.recipient = recipient;
        receipt.body_hash = body_hash;
        receipt.bump = ctx.bumps.receipt;
        emit!(MessageReceived { source_domain, nonce, sender, recipient, body_hash });
        Ok(())
    }

    pub fn mint_and_withdraw(
        ctx: Context<MintAndWithdraw>, source_domain: u32, nonce: u64, amount: u64,
    ) -> Result<()> {
        require!(!ctx.accounts.state.paused, BridgeError::Paused);
        require_keys_eq!(ctx.accounts.attester.key(), ctx.accounts.state.attester, BridgeError::UnauthorizedAttester);
        require!(!ctx.accounts.receipt.consumed, BridgeError::AlreadyConsumed);
        require!(ctx.accounts.receipt.source_domain == source_domain && ctx.accounts.receipt.nonce == nonce, BridgeError::InvalidReceipt);
        require_keys_eq!(ctx.accounts.receipt.recipient, ctx.accounts.recipient.key(), BridgeError::InvalidRecipient);
        let seeds: &[&[u8]] = &[b"state", &[ctx.accounts.state.bump]];
        token::mint_to(CpiContext::new_with_signer(ctx.accounts.token_program.to_account_info(), MintTo {
            mint: ctx.accounts.bridge_mint.to_account_info(), to: ctx.accounts.recipient_token.to_account_info(),
            authority: ctx.accounts.state.to_account_info(),
        }, &[seeds]), amount)?;
        ctx.accounts.receipt.consumed = true;
        emit!(MintAndWithdrawEvent { source_domain, nonce, recipient: ctx.accounts.recipient.key(), amount });
        Ok(())
    }

    pub fn pause(ctx: Context<AdminOnly>) -> Result<()> { ctx.accounts.state.paused = true; Ok(()) }
    pub fn unpause(ctx: Context<AdminOnly>) -> Result<()> { ctx.accounts.state.paused = false; Ok(()) }
    pub fn update_attester(ctx: Context<AdminOnly>, attester: Pubkey) -> Result<()> { ctx.accounts.state.attester = attester; Ok(()) }
    pub fn transfer_ownership(ctx: Context<AdminOnly>, pending_admin: Pubkey) -> Result<()> { ctx.accounts.state.pending_admin = pending_admin; Ok(()) }
    pub fn accept_ownership(ctx: Context<AcceptOwnership>) -> Result<()> {
        require_keys_eq!(ctx.accounts.pending_admin.key(), ctx.accounts.state.pending_admin, BridgeError::Unauthorized);
        ctx.accounts.state.admin = ctx.accounts.pending_admin.key();
        ctx.accounts.state.pending_admin = Pubkey::default();
        Ok(())
    }
}

fn deposit(ctx: Context<DepositForBurn>, amount: u64, destination_domain: u32,
    mint_recipient: [u8; 32], destination_caller: [u8; 32]) -> Result<()> {
    require!(!ctx.accounts.state.paused, BridgeError::Paused);
    require!(amount > 0, BridgeError::ZeroAmount);
    token::burn(CpiContext::new(ctx.accounts.token_program.to_account_info(), Burn {
        mint: ctx.accounts.burn_mint.to_account_info(), from: ctx.accounts.burn_token.to_account_info(),
        authority: ctx.accounts.depositor.to_account_info(),
    }), amount)?;
    message_transmitter::cpi::send_message(
        CpiContext::new(ctx.accounts.message_transmitter_program.to_account_info(),
            message_transmitter::cpi::accounts::SendMessage {
                state: ctx.accounts.transmitter_state.to_account_info(),
                sender: ctx.accounts.depositor.to_account_info(),
                payer: ctx.accounts.depositor.to_account_info(),
                message: ctx.accounts.transmitter_message.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
            }),
        destination_domain,
        mint_recipient,
        destination_caller,
        amount.to_le_bytes().to_vec(),
    )?;
    let nonce = ctx.accounts.state.next_nonce;
    ctx.accounts.state.next_nonce = nonce.checked_add(1).ok_or(BridgeError::Overflow)?;
    let message = &mut ctx.accounts.message;
    message.nonce = nonce; message.source_domain = ctx.accounts.state.local_domain;
    message.destination_domain = destination_domain; message.sender = ctx.accounts.depositor.key();
    message.burn_mint = ctx.accounts.burn_mint.key(); message.amount = amount;
    message.mint_recipient = mint_recipient; message.destination_caller = destination_caller;
    message.finalized = false; message.replacement_count = 0; message.bump = ctx.bumps.message;
    emit!(DepositForBurnEvent { nonce, burn_token: message.burn_mint, amount,
        depositor: message.sender, destination_domain, mint_recipient, destination_caller });
    emit!(MessageSent { nonce, source_domain: message.source_domain, destination_domain,
        sender: message.sender, recipient: mint_recipient, destination_caller, body: amount.to_le_bytes().to_vec() });
    Ok(())
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer = admin, space = 8 + BridgeState::INIT_SPACE, seeds=[b"state"], bump)]
    pub state: Account<'info, BridgeState>,
    #[account(mut)] pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct DepositForBurn<'info> {
    #[account(mut, seeds=[b"state"], bump=state.bump)] pub state: Account<'info, BridgeState>,
    #[account(mut)] pub depositor: Signer<'info>,
    #[account(mut, constraint=burn_token.owner == depositor.key(), constraint=burn_token.mint == burn_mint.key())]
    pub burn_token: Account<'info, TokenAccount>,
    #[account(mut)] pub burn_mint: Account<'info, Mint>,
    #[account(init, payer=depositor, space=8 + BurnMessage::INIT_SPACE,
        seeds=[b"burn_message", state.key().as_ref(), &state.next_nonce.to_le_bytes()], bump)]
    pub message: Account<'info, BurnMessage>,
    pub token_program: Program<'info, Token>,
    /// CHECK: validated by the Message Transmitter CPI.
    #[account(mut)] pub transmitter_state: UncheckedAccount<'info>,
    /// CHECK: initialized and validated by the Message Transmitter CPI.
    #[account(mut)] pub transmitter_message: UncheckedAccount<'info>,
    pub message_transmitter_program: Program<'info, MessageTransmitter>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ReplaceMessage<'info> {
    #[account(mut, has_one=sender)] pub message: Account<'info, BurnMessage>,
    pub sender: Signer<'info>,
}

#[derive(Accounts)]
pub struct SendMessage<'info> {
    #[account(mut, seeds=[b"state"], bump=state.bump)] pub state: Account<'info, BridgeState>,
    #[account(mut)] pub sender: Signer<'info>,
    #[account(init, payer=sender, space=8 + GenericMessage::INIT_SPACE,
        seeds=[b"message", state.key().as_ref(), &state.next_nonce.to_le_bytes()], bump)]
    pub message: Account<'info, GenericMessage>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(source_domain: u32, nonce: u64)]
pub struct ReceiveMessage<'info> {
    #[account(seeds=[b"state"], bump=state.bump)] pub state: Account<'info, BridgeState>,
    pub attester: Signer<'info>,
    #[account(mut)] pub payer: Signer<'info>,
    #[account(init, payer=payer, space=8 + Receipt::INIT_SPACE,
        seeds=[b"receipt", &source_domain.to_le_bytes(), &nonce.to_le_bytes()], bump)]
    pub receipt: Account<'info, Receipt>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct MintAndWithdraw<'info> {
    #[account(seeds=[b"state"], bump=state.bump)] pub state: Account<'info, BridgeState>,
    pub attester: Signer<'info>,
    #[account(mut)] pub receipt: Account<'info, Receipt>,
    /// CHECK: constrained to the receipt recipient.
    pub recipient: UncheckedAccount<'info>,
    #[account(mut, constraint=bridge_mint.mint_authority == anchor_spl::token::spl_token::state::COption::Some(state.key()))]
    pub bridge_mint: Account<'info, Mint>,
    #[account(mut, constraint=recipient_token.owner == recipient.key(), constraint=recipient_token.mint == bridge_mint.key())]
    pub recipient_token: Account<'info, TokenAccount>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    #[account(mut, seeds=[b"state"], bump=state.bump, has_one=admin)] pub state: Account<'info, BridgeState>,
    pub admin: Signer<'info>,
}

#[derive(Accounts)]
pub struct AcceptOwnership<'info> {
    #[account(mut, seeds=[b"state"], bump=state.bump)] pub state: Account<'info, BridgeState>,
    pub pending_admin: Signer<'info>,
}

#[account]
#[derive(InitSpace)]
pub struct BridgeState { pub admin: Pubkey, pub pending_admin: Pubkey, pub attester: Pubkey,
    pub local_domain: u32, pub next_nonce: u64, pub paused: bool, pub bump: u8 }

#[account]
#[derive(InitSpace)]
pub struct BurnMessage { pub nonce: u64, pub source_domain: u32, pub destination_domain: u32,
    pub sender: Pubkey, pub burn_mint: Pubkey, pub amount: u64, pub mint_recipient: [u8;32],
    pub destination_caller: [u8;32], pub finalized: bool, pub replacement_count: u32, pub bump: u8 }

#[account]
#[derive(InitSpace)]
pub struct GenericMessage { pub nonce: u64, pub source_domain: u32, pub destination_domain: u32,
    pub sender: Pubkey, pub recipient: [u8;32], pub destination_caller: [u8;32],
    #[max_len(1024)] pub body: Vec<u8>, pub finalized: bool, pub bump: u8 }

#[account]
#[derive(InitSpace)]
pub struct Receipt { pub source_domain: u32, pub nonce: u64, pub sender: [u8;32],
    pub recipient: Pubkey, pub body_hash: [u8;32], pub consumed: bool, pub bump: u8 }

#[event] pub struct DepositForBurnEvent { pub nonce: u64, pub burn_token: Pubkey, pub amount: u64,
    pub depositor: Pubkey, pub destination_domain: u32, pub mint_recipient: [u8;32], pub destination_caller: [u8;32] }
#[event] pub struct MessageSent { pub nonce: u64, pub source_domain: u32, pub destination_domain: u32,
    pub sender: Pubkey, pub recipient: [u8;32], pub destination_caller: [u8;32], pub body: Vec<u8> }
#[event] pub struct MessageReplaced { pub nonce: u64, pub sender: Pubkey,
    pub mint_recipient: [u8;32], pub destination_caller: [u8;32] }
#[event] pub struct MessageReceived { pub source_domain: u32, pub nonce: u64,
    pub sender: [u8;32], pub recipient: Pubkey, pub body_hash: [u8;32] }
#[event] pub struct MintAndWithdrawEvent { pub source_domain: u32, pub nonce: u64, pub recipient: Pubkey, pub amount: u64 }

#[error_code]
pub enum BridgeError { #[msg("Bridge is paused")] Paused, #[msg("Amount is zero")] ZeroAmount,
    #[msg("Arithmetic overflow")] Overflow, #[msg("Invalid destination caller")] InvalidCaller,
    #[msg("Message too large")] MessageTooLarge, #[msg("Unauthorized attester")] UnauthorizedAttester,
    #[msg("Unauthorized")] Unauthorized, #[msg("Message already finalized")] AlreadyFinalized,
    #[msg("Receipt already consumed")] AlreadyConsumed, #[msg("Invalid receipt")] InvalidReceipt,
    #[msg("Invalid recipient")] InvalidRecipient }
