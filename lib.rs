use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

declare_id!("GQkWaYk52wYayjGXPcMHEh6KsskLrX5c2Sh9BXbjSKwY");

// Dedicated ICO Configuration
pub const ADMIN_PUBKEY: Pubkey = pubkey!("EjrnCBn8RvrrnLNTdSseqsS6dkQ6o7gn8HrxKdTo8Jdg");
pub const PRICE_PER_WHOLE_TOKEN_LAMPORTS: u64 = 1_000_000; // 0.001 SOL per whole token
pub const MAX_WHOLE_TOKENS_PER_WALLET: u64 = 10_000;

#[program]
pub mod solana_ico {
    use super::*;

    pub fn create_ico_ata(
        ctx: Context<CreateICO>, 
        ico_amount_whole: u64, 
        start_time: i64, 
        end_time: i64
    ) -> Result<()> {
        let clock = Clock::get()?;
        require!(end_time > start_time, ErrorCode::InvalidTimeRange);
        // 60-second grace window for network drift/latency
        require!(start_time >= clock.unix_timestamp - 60, ErrorCode::StartTimeInPast);

        let decimals = ctx.accounts.ico_mint.decimals;
        let multiplier = 10u64.pow(decimals as u32);
        let raw_ico_amount = ico_amount_whole.checked_mul(multiplier).ok_or(ErrorCode::ArithmeticOverflow)?;

        // Initial funding from admin to program vault
        let cpi_ctx = CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            token::Transfer {
                from: ctx.accounts.ico_ata_for_admin.to_account_info(),
                to: ctx.accounts.ico_ata_for_ico_program.to_account_info(),
                authority: ctx.accounts.admin.to_account_info(),
            },
        );
        token::transfer(cpi_ctx, raw_ico_amount)?;

        let data = &mut ctx.accounts.data;
        data.admin = ctx.accounts.admin.key();
        data.ico_mint = ctx.accounts.ico_mint.key();
        data.total_tokens = raw_ico_amount;
        data.tokens_sold = 0;
        data.decimals = decimals;
        data.start_time = start_time;
        data.end_time = end_time;
        data.is_active = true;

        Ok(())
    }

    pub fn buy_tokens(ctx: Context<BuyTokens>, raw_amount_to_buy: u64) -> Result<()> {
        let clock = Clock::get()?;
        let data = &ctx.accounts.data;

        // 1. Lifecycle & Guard Checks
        require!(raw_amount_to_buy > 0, ErrorCode::AmountTooSmall);
        require!(data.is_active, ErrorCode::IcoInactive);
        require!(clock.unix_timestamp >= data.start_time, ErrorCode::IcoNotStarted);
        require!(clock.unix_timestamp <= data.end_time, ErrorCode::IcoEnded);

        // 2. Unit Algebra & Dusting Protection
        let multiplier = 10u128.pow(data.decimals as u32);
        let sol_amount = (raw_amount_to_buy as u128)
            .checked_mul(PRICE_PER_WHOLE_TOKEN_LAMPORTS as u128)
            .ok_or(ErrorCode::ArithmeticOverflow)?
            .checked_div(multiplier)
            .ok_or(ErrorCode::ArithmeticOverflow)? as u64;

        require!(sol_amount > 0, ErrorCode::AmountTooSmall);

        // 3. Per-Wallet Cap Enforcement
        let cap_raw = MAX_WHOLE_TOKENS_PER_WALLET
            .checked_mul(10u64.pow(data.decimals as u32))
            .ok_or(ErrorCode::ArithmeticOverflow)?;
        
        let new_total = ctx.accounts.buyer_stats.amount_bought
            .checked_add(raw_amount_to_buy)
            .ok_or(ErrorCode::ArithmeticOverflow)?;
        require!(new_total <= cap_raw, ErrorCode::ExceedsWalletCap);

        // 4. Token Transfer (PDA-signed Vault -> User)
        pda_token_transfer(
            ctx.accounts.token_program.to_account_info(),
            ctx.accounts.ico_ata_for_ico_program.to_account_info(),
            ctx.accounts.user_token_account.to_account_info(),
            ctx.accounts.data.to_account_info(),
            &ctx.accounts.ico_mint.key(),
            ctx.bumps.data,
            raw_amount_to_buy,
        )?;

        // 5. SOL Transfer (User -> Admin)
        let sol_ix = anchor_lang::solana_program::system_instruction::transfer(
            &ctx.accounts.user.key(),
            &ctx.accounts.admin.key(),
            sol_amount,
        );
        anchor_lang::solana_program::program::invoke(&sol_ix, &[
            ctx.accounts.user.to_account_info(),
            ctx.accounts.admin.to_account_info(),
        ])?;

        // 6. State Updates & Events
        ctx.accounts.buyer_stats.amount_bought = new_total;
        ctx.accounts.data.tokens_sold = ctx.accounts.data.tokens_sold
            .checked_add(raw_amount_to_buy)
            .ok_or(ErrorCode::ArithmeticOverflow)?;

        emit!(TokensPurchased {
            buyer: ctx.accounts.user.key(),
            raw_amount: raw_amount_to_buy,
            sol_paid: sol_amount,
            timestamp: clock.unix_timestamp,
        });

        Ok(())
    }

    pub fn withdraw_unsold(ctx: Context<WithdrawUnsold>) -> Result<()> {
        let clock = Clock::get()?;
        let amount = ctx.accounts.ico_ata_for_ico_program.amount;
        let end_time = ctx.accounts.data.end_time; // Cache before close

        require!(clock.unix_timestamp > end_time, ErrorCode::IcoStillRunning);
        require!(amount > 0, ErrorCode::NothingToWithdraw);

        // 1. Reclaim tokens to admin
        pda_token_transfer(
            ctx.accounts.token_program.to_account_info(),
            ctx.accounts.ico_ata_for_ico_program.to_account_info(),
            ctx.accounts.admin_token_account.to_account_info(),
            ctx.accounts.data.to_account_info(),
            &ctx.accounts.ico_mint.key(),
            ctx.bumps.data,
            amount,
        )?;

        // 2. Close Vault ATA to reclaim its rent
        token::close_account(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                token::CloseAccount {
                    account: ctx.accounts.ico_ata_for_ico_program.to_account_info(),
                    destination: ctx.accounts.admin.to_account_info(),
                    authority: ctx.accounts.data.to_account_info(),
                },
                &[&[b"ico_state", ctx.accounts.ico_mint.key().as_ref(), &[ctx.bumps.data]][..]],
            )
        )?;

        Ok(())
    }

    pub fn toggle_pause(ctx: Context<AdminOnly>, is_active: bool) -> Result<()> {
        ctx.accounts.data.is_active = is_active;
        emit!(IcoStatusChanged {
            is_active,
            timestamp: Clock::get()?.unix_timestamp,
        });
        Ok(())
    }
}

// ============================================================
// Context Structs
// ============================================================

#[derive(Accounts)]
pub struct CreateICO<'info> {
    #[account(mut, address = ADMIN_PUBKEY)]
    pub admin: Signer<'info>,
    pub ico_mint: Account<'info, Mint>,

    #[account(
        init,
        payer = admin,
        space = 8 + IcoData::INIT_SPACE,
        seeds = [b"ico_state", ico_mint.key().as_ref()],
        bump
    )]
    pub data: Account<'info, IcoData>,

    #[account(
        init_if_needed,
        payer = admin,
        associated_token::mint = ico_mint,
        associated_token::authority = data,
    )]
    pub ico_ata_for_ico_program: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = ico_mint,
        associated_token::authority = admin,
    )]
    pub ico_ata_for_admin: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct BuyTokens<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// CHECK: System-owned admin wallet
    #[account(mut, address = ADMIN_PUBKEY, owner = system_program.key())]
    pub admin: AccountInfo<'info>,

    pub ico_mint: Account<'info, Mint>,

    #[account(
        init_if_needed,
        payer = user,
        space = 8 + BuyerStats::INIT_SPACE,
        seeds = [b"buyer", user.key().as_ref(), ico_mint.key().as_ref()],
        bump
    )]
    pub buyer_stats: Account<'info, BuyerStats>,

    #[account(
        mut,
        seeds = [b"ico_state", ico_mint.key().as_ref()],
        bump,
        has_one = admin @ ErrorCode::InvalidAdmin,
    )]
    pub data: Account<'info, IcoData>,

    #[account(
        mut,
        associated_token::mint = ico_mint,
        associated_token::authority = data,
    )]
    pub ico_ata_for_ico_program: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = ico_mint,
        associated_token::authority = user,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct WithdrawUnsold<'info> {
    #[account(mut, address = ADMIN_PUBKEY)]
    pub admin: Signer<'info>,

    #[account(
        mut,
        close = admin,
        seeds = [b"ico_state", ico_mint.key().as_ref()],
        bump,
        has_one = admin @ ErrorCode::InvalidAdmin,
    )]
    pub data: Account<'info, IcoData>,

    pub ico_mint: Account<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = ico_mint,
        associated_token::authority = data,
    )]
    pub ico_ata_for_ico_program: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = ico_mint,
        associated_token::authority = admin,
    )]
    pub admin_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct AdminOnly<'info> {
    #[account(mut, address = ADMIN_PUBKEY)]
    pub admin: Signer<'info>,
    pub ico_mint: Account<'info, Mint>,
    #[account(
        mut,
        seeds = [b"ico_state", ico_mint.key().as_ref()],
        bump,
        has_one = admin @ ErrorCode::InvalidAdmin,
    )]
    pub data: Account<'info, IcoData>,
}

// ============================================================
// State & Helpers
// ============================================================

#[account]
#[derive(InitSpace)]
pub struct IcoData {
    pub admin: Pubkey,
    pub ico_mint: Pubkey,
    pub total_tokens: u64,
    pub tokens_sold: u64,
    pub decimals: u8,
    pub start_time: i64,
    pub end_time: i64,
    pub is_active: bool,
}

#[account]
#[derive(InitSpace)]
pub struct BuyerStats {
    pub amount_bought: u64,
}

#[event]
pub struct TokensPurchased {
    pub buyer: Pubkey,
    pub raw_amount: u64,
    pub sol_paid: u64,
    pub timestamp: i64,
}

#[event]
pub struct IcoStatusChanged {
    pub is_active: bool,
    pub timestamp: i64,
}

// Generic PDA Signer Helper
fn pda_token_transfer<'info>(
    token_program: AccountInfo<'info>,
    from: AccountInfo<'info>,
    to: AccountInfo<'info>,
    authority: AccountInfo<'info>,
    ico_mint_key: &Pubkey,
    bump: u8,
    amount: u64,
) -> Result<()> {
    let seeds = &[b"ico_state", ico_mint_key.as_ref(), &[bump]];
    let signer = &[&seeds[..]];

    token::transfer(
        CpiContext::new_with_signer(
            token_program,
            token::Transfer { from, to, authority },
            signer,
        ),
        amount,
    )
}

#[error_code]
pub enum ErrorCode {
    #[msg("Arithmetic overflow occurred")]
    ArithmeticOverflow,
    #[msg("Invalid Admin")]
    InvalidAdmin,
    #[msg("Amount too small to purchase or withdraw")]
    AmountTooSmall,
    #[msg("ICO is not active or has been paused")]
    IcoInactive,
    #[msg("ICO has not started yet")]
    IcoNotStarted,
    #[msg("ICO has already ended")]
    IcoEnded,
    #[msg("Provided start time is too far in the past")]
    StartTimeInPast,
    #[msg("End time must be after start time")]
    InvalidTimeRange,
    #[msg("Purchase exceeds maximum per-wallet cap")]
    ExceedsWalletCap,
    #[msg("ICO is still running; cannot withdraw unsold tokens")]
    IcoStillRunning,
    #[msg("No tokens available to withdraw")]
    NothingToWithdraw,
    #[msg("Insufficient tokens in the ICO vault")]
    InsufficientIcoLiquidity,
}