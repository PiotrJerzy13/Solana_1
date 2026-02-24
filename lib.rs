use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount};

declare_id!("GQkWaYk52wYayjGXPcMHEh6KsskLrX5c2Sh9BXbjSKwY");

pub const LAMPORTS_PER_TOKEN: u64 = 1_000_000;
pub const TOKEN_DECIMALS: u64 = 1_000_000_000;

#[error_code]
pub enum ErrorCode {
    #[msg("Arithmetic overflow occurred")]
    ArithmeticOverflow,
    #[msg("Invalid Admin")]
    InvalidAdmin,
}

#[program]
pub mod solana_ico {
    use super::*;

    pub fn create_ico_ata(ctx: Context<CreateICO>, ico_amount: u64) -> Result<()> {
        let raw_ico_amount = ico_amount
            .checked_mul(TOKEN_DECIMALS)
            .ok_or(ErrorCode::ArithmeticOverflow)?;

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

        msg!("ICO created with {} tokens", raw_ico_amount);
        Ok(())
    }

    pub fn deposit_ico_in_ata(ctx: Context<DepositIcoATA>, ico_amount: u64) -> Result<()> {
        let raw_ico_amount = ico_amount
            .checked_mul(TOKEN_DECIMALS)
            .ok_or(ErrorCode::ArithmeticOverflow)?;

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
        data.total_tokens = data
            .total_tokens
            .checked_add(raw_ico_amount)
            .ok_or(ErrorCode::ArithmeticOverflow)?;

        msg!("Deposited {} tokens", raw_ico_amount);
        Ok(())
    }

    pub fn buy_tokens(ctx: Context<BuyTokens>, ico_ata_bump: u8, token_amount: u64) -> Result<()> {
        let raw_token_amount = token_amount
            .checked_mul(TOKEN_DECIMALS)
            .ok_or(ErrorCode::ArithmeticOverflow)?;

        let sol_amount = token_amount
            .checked_mul(LAMPORTS_PER_TOKEN)
            .ok_or(ErrorCode::ArithmeticOverflow)?;

        // SOL transfer: user → admin
        let ix = anchor_lang::solana_program::system_instruction::transfer(
            &ctx.accounts.user.key(),
            &ctx.accounts.admin.key(),
            sol_amount,
        );
        anchor_lang::solana_program::program::invoke(
            &ix,
            &[
                ctx.accounts.user.to_account_info(),
                ctx.accounts.admin.to_account_info(),
            ],
        )?;

        // Token transfer: program ATA → user ATA (PDA signs)
        let ico_mint_key = ctx.accounts.ico_mint.key();
        let seeds = &[b"ico_state".as_ref(), ico_mint_key.as_ref(), &[ico_ata_bump]];
        let signer = &[&seeds[..]];

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            token::Transfer {
                from: ctx.accounts.ico_ata_for_ico_program.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.data.to_account_info(), // PDA is authority
            },
            signer,
        );
        token::transfer(cpi_ctx, raw_token_amount)?;

        let data = &mut ctx.accounts.data;
        data.tokens_sold = data
            .tokens_sold
            .checked_add(raw_token_amount)
            .ok_or(ErrorCode::ArithmeticOverflow)?;

        msg!("Sold {} tokens for {} lamports", raw_token_amount, sol_amount);
        Ok(())
    }
}

// ============================================================
// Context Structs
// ============================================================

#[derive(Accounts)]
pub struct CreateICO<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    pub ico_mint: Account<'info, Mint>,

    #[account(mut)]
    pub ico_ata_for_admin: Account<'info, TokenAccount>,

    // TODO: pre-create this ATA manually before calling this instruction
    // It must be owned by the `data` PDA and use `ico_mint` as its mint
    #[account(mut)]
    pub ico_ata_for_ico_program: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = admin,
        space = IcoData::SIZE,
        seeds = [b"ico_state", ico_mint.key().as_ref()],
        bump,
    )]
    pub data: Account<'info, IcoData>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct DepositIcoATA<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    pub ico_mint: Account<'info, Mint>,

    #[account(mut)]
    pub ico_ata_for_admin: Account<'info, TokenAccount>,

    #[account(mut)]
    pub ico_ata_for_ico_program: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"ico_state", ico_mint.key().as_ref()],
        bump,
        has_one = admin @ ErrorCode::InvalidAdmin,
    )]
    pub data: Account<'info, IcoData>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(ico_ata_bump: u8)]
pub struct BuyTokens<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// CHECK: receives SOL, verified against data.admin
    #[account(
        mut,
        constraint = admin.key() == data.admin @ ErrorCode::InvalidAdmin
    )]
    pub admin: AccountInfo<'info>,

    pub ico_mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds = [b"ico_state", ico_mint.key().as_ref()],
        bump,
    )]
    pub data: Account<'info, IcoData>,

    #[account(mut)]
    pub ico_ata_for_ico_program: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

// ============================================================
// State
// ============================================================

#[account]
pub struct IcoData {
    pub admin: Pubkey,      // 32
    pub ico_mint: Pubkey,   // 32
    pub total_tokens: u64,  // 8
    pub tokens_sold: u64,   // 8
}

impl IcoData {
    pub const SIZE: usize = 8 + 32 + 32 + 8 + 8; // = 88
}