use anchor_lang::prelude::*;

declare_id!("MsgT111111111111111111111111111111111111111");

#[program]
pub mod message_transmitter {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, local_domain: u32, attester: Pubkey) -> Result<()> {
        let state = &mut ctx.accounts.state;
        state.admin = ctx.accounts.admin.key();
        state.attester = attester;
        state.local_domain = local_domain;
        state.next_available_nonce = 0;
        state.paused = false;
        state.bump = ctx.bumps.state;
        Ok(())
    }

    pub fn send_message(ctx: Context<SendMessage>, destination_domain: u32,
        recipient: [u8; 32], destination_caller: [u8; 32], message_body: Vec<u8>) -> Result<()> {
        require!(!ctx.accounts.state.paused, TransmitterError::Paused);
        require!(message_body.len() <= 1024, TransmitterError::MessageTooLarge);
        let nonce = ctx.accounts.state.next_available_nonce;
        ctx.accounts.state.next_available_nonce = nonce.checked_add(1).ok_or(TransmitterError::Overflow)?;
        let message = &mut ctx.accounts.message;
        message.version = 0;
        message.source_domain = ctx.accounts.state.local_domain;
        message.destination_domain = destination_domain;
        message.nonce = nonce;
        message.sender = ctx.accounts.sender.key();
        message.recipient = recipient;
        message.destination_caller = destination_caller;
        message.message_body = message_body.clone();
        message.bump = ctx.bumps.message;
        emit!(MessageSent { version: 0, source_domain: message.source_domain,
            destination_domain, nonce, sender: message.sender, recipient,
            destination_caller, message_body });
        Ok(())
    }

    pub fn replace_message(ctx: Context<ReplaceMessage>, recipient: [u8; 32],
        destination_caller: [u8; 32], message_body: Vec<u8>) -> Result<()> {
        require!(message_body.len() <= 1024, TransmitterError::MessageTooLarge);
        let message = &mut ctx.accounts.message;
        message.recipient = recipient;
        message.destination_caller = destination_caller;
        message.message_body = message_body.clone();
        emit!(MessageReplaced { nonce: message.nonce, sender: message.sender,
            recipient, destination_caller, message_body });
        Ok(())
    }

    pub fn receive_message(ctx: Context<ReceiveMessage>, source_domain: u32, nonce: u64,
        sender: [u8; 32], recipient: Pubkey, message_hash: [u8; 32]) -> Result<()> {
        require!(!ctx.accounts.state.paused, TransmitterError::Paused);
        require_keys_eq!(ctx.accounts.attester.key(), ctx.accounts.state.attester, TransmitterError::Unauthorized);
        let receipt = &mut ctx.accounts.receipt;
        receipt.source_domain = source_domain; receipt.nonce = nonce; receipt.sender = sender;
        receipt.recipient = recipient; receipt.message_hash = message_hash; receipt.bump = ctx.bumps.receipt;
        emit!(MessageReceived { source_domain, nonce, sender, recipient, message_hash });
        Ok(())
    }

    pub fn pause(ctx: Context<AdminOnly>) -> Result<()> { ctx.accounts.state.paused = true; Ok(()) }
    pub fn unpause(ctx: Context<AdminOnly>) -> Result<()> { ctx.accounts.state.paused = false; Ok(()) }
    pub fn update_attester(ctx: Context<AdminOnly>, attester: Pubkey) -> Result<()> { ctx.accounts.state.attester = attester; Ok(()) }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(init, payer=admin, space=8 + TransmitterState::INIT_SPACE, seeds=[b"transmitter"], bump)]
    pub state: Account<'info, TransmitterState>,
    #[account(mut)] pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct SendMessage<'info> {
    #[account(mut, seeds=[b"transmitter"], bump=state.bump)] pub state: Account<'info, TransmitterState>,
    pub sender: Signer<'info>,
    #[account(mut)] pub payer: Signer<'info>,
    #[account(init, payer=payer, space=8 + OutboundMessage::INIT_SPACE,
        seeds=[b"message", state.key().as_ref(), &state.next_available_nonce.to_le_bytes()], bump)]
    pub message: Account<'info, OutboundMessage>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ReplaceMessage<'info> {
    #[account(mut, has_one=sender)] pub message: Account<'info, OutboundMessage>,
    pub sender: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(source_domain: u32, nonce: u64)]
pub struct ReceiveMessage<'info> {
    #[account(seeds=[b"transmitter"], bump=state.bump)] pub state: Account<'info, TransmitterState>,
    pub attester: Signer<'info>,
    #[account(mut)] pub payer: Signer<'info>,
    #[account(init, payer=payer, space=8 + Receipt::INIT_SPACE,
        seeds=[b"used_nonce", &source_domain.to_le_bytes(), &nonce.to_le_bytes()], bump)]
    pub receipt: Account<'info, Receipt>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    #[account(mut, seeds=[b"transmitter"], bump=state.bump, has_one=admin)] pub state: Account<'info, TransmitterState>,
    pub admin: Signer<'info>,
}

#[account]
#[derive(InitSpace)]
pub struct TransmitterState { pub admin: Pubkey, pub attester: Pubkey, pub local_domain: u32,
    pub next_available_nonce: u64, pub paused: bool, pub bump: u8 }

#[account]
#[derive(InitSpace)]
pub struct OutboundMessage { pub version: u32, pub source_domain: u32, pub destination_domain: u32,
    pub nonce: u64, pub sender: Pubkey, pub recipient: [u8;32], pub destination_caller: [u8;32],
    #[max_len(1024)] pub message_body: Vec<u8>, pub bump: u8 }

#[account]
#[derive(InitSpace)]
pub struct Receipt { pub source_domain: u32, pub nonce: u64, pub sender: [u8;32],
    pub recipient: Pubkey, pub message_hash: [u8;32], pub bump: u8 }

#[event]
pub struct MessageSent { pub version: u32, pub source_domain: u32, pub destination_domain: u32,
    pub nonce: u64, pub sender: Pubkey, pub recipient: [u8;32], pub destination_caller: [u8;32], pub message_body: Vec<u8> }
#[event]
pub struct MessageReplaced { pub nonce: u64, pub sender: Pubkey, pub recipient: [u8;32],
    pub destination_caller: [u8;32], pub message_body: Vec<u8> }
#[event]
pub struct MessageReceived { pub source_domain: u32, pub nonce: u64, pub sender: [u8;32],
    pub recipient: Pubkey, pub message_hash: [u8;32] }

#[error_code]
pub enum TransmitterError { #[msg("Message transmitter is paused")] Paused,
    #[msg("Message body too large")] MessageTooLarge, #[msg("Arithmetic overflow")] Overflow,
    #[msg("Unauthorized") ] Unauthorized }
