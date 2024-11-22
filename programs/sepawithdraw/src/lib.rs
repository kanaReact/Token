#![allow(unused)]
use anchor_lang::prelude::*;
use anchor_lang::solana_program::clock;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{self, Mint, TokenAccount, Transfer},
};
declare_id!("C1dGXHWZ1TyFQjkfQcqjsckcYuhak63X4PCn2rkXkMGL");

const PUB_POOL_SEEDS: &[u8] = b"public_pool";
const RES_POOL_SEEDS: &[u8] = b"reserve_pool";
const VEST_SEED: &[u8] = b"vesting";

#[program]
pub mod sepawithdraw {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        let state = &mut ctx.accounts.state;
        state.admin = ctx.accounts.admin.key();
        state.mint = ctx.accounts.mint.key();
        state.payment_mint = ctx.accounts.payment_mint.key();
        state.pub_supply = 0;
        state.reserve_supply = 0;
        state.current_active_phase = u8::MAX;
        msg!("PDA initialized");
        msg!("Admin set to: {}", state.admin);
        Ok(())
    }

    pub fn dogdistribution(ctx: Context<Distoken>, totalsupply: u64) -> Result<()> {
        let token_program = ctx.accounts.token_program.to_account_info();
        let admin = ctx.accounts.admin.to_account_info();
        let owner_ata = ctx.accounts.admin_ata.to_account_info();
        let mut totalamount = totalsupply;

        let pubsup_ata = ctx.accounts.pubsup_ata.to_account_info();
        let reserve_ata = ctx.accounts.reserve_pool_ata.to_account_info();
        let state = &mut ctx.accounts.state;

        // Distribute 50% each to public and reserve pools
        let pub_supply = (totalamount.checked_mul(50).unwrap())
            .checked_div(100)
            .unwrap();
        let reserve_supply = (totalamount.checked_mul(50).unwrap())
            .checked_div(100)
            .unwrap();

        state.pub_supply = pub_supply;
        state.reserve_supply = reserve_supply;

        // Transfer public supply to public supply ATA
        let instruction = anchor_spl::token::Transfer {
            authority: admin.to_account_info(),
            from: owner_ata.to_account_info(),
            to: pubsup_ata.to_account_info(),
        };
        token::transfer(
            CpiContext::new(token_program.to_account_info(), instruction),
            pub_supply,
        )?;
        totalamount -= pub_supply;
        msg!("Public supply transfer done...");

        // Transfer reserve supply to reserve supply ATA
        let instruction1 = anchor_spl::token::Transfer {
            authority: admin.to_account_info(),
            from: owner_ata.to_account_info(),
            to: reserve_ata.to_account_info(),
        };
        token::transfer(
            CpiContext::new(token_program.to_account_info(), instruction1),
            reserve_supply,
        )?;
        totalamount -= reserve_supply;
        msg!("Reserve supply transfer done...");

        // Set presale round prices and balances
        let r1_supply = (pub_supply.checked_mul(60).unwrap())
            .checked_div(100)
            .unwrap();
        let r2_supply = (pub_supply.checked_mul(20).unwrap())
            .checked_div(100)
            .unwrap();
        let r3_supply = (pub_supply.checked_mul(20).unwrap())
            .checked_div(100)
            .unwrap();

        state.rounds[0].price = 0.002;
        state.rounds[1].price = 0.003;
        state.rounds[2].price = 0.004;
        state.rounds[0].balance = r1_supply;
        state.rounds[1].balance = r2_supply;
        state.rounds[2].balance = r3_supply;

        msg!("Total amount remaining: {}", totalamount);
        Ok(())
    }

    pub fn start_round(ctx: Context<StartRound>, round: u8) -> Result<()> {
        let state = &mut ctx.accounts.state;
        let clock = clock::Clock::get().unwrap();

        require!(
            ctx.accounts.admin.key() == state.admin,
            CustomError::Unauthorized
        );

        // Ensure the round number is valid
        if round as usize >= state.rounds.len() {
            return Err(CustomError::InvalidRound.into());
        }

        // Ensure rounds are started in increasing order
        if state.current_active_phase != u8::MAX && round <= state.current_active_phase {
            return Err(CustomError::InvalidRoundOrder.into());
        }

        // Collect unsold tokens from the currently active round (if any)
        if state.current_active_phase != u8::MAX {
            let active_phase = state.current_active_phase as usize;
            let active_round = &mut state.rounds[active_phase];

            active_round.active = false;
            active_round.end_time = clock.unix_timestamp;
            let total_unsold_tokens = active_round.balance;
            active_round.balance = 0;

            // Add unsold tokens to the public pool
            state.pub_supply = state
                .pub_supply
                .checked_add(total_unsold_tokens)
                .unwrap();
        }

        // Start the new round
        let new_start_time = clock.unix_timestamp;
        let new_end_time = new_start_time + 1 * 24 * 60 * 60; // one-day duration
        let new_round = &mut state.rounds[round as usize];
        new_round.active = true;
        new_round.start_time = new_start_time;
        new_round.end_time = new_end_time;

        state.current_active_phase = round;

        msg!(
            "Round {} started. Start time: {}, End time: {}",
            round,
            new_start_time,
            new_end_time
        );
        Ok(())
    }

    pub fn purchasenow(ctx: Context<Purchasenow>, pay_amount: u64, round: u8) -> Result<()> {
        let token_program = ctx.accounts.token_program.to_account_info();
        let clock = clock::Clock::get().unwrap();
        let buyer = &ctx.accounts.buyer.to_account_info();
        let state = &mut ctx.accounts.state;

        // Ensure the round number is valid
        if round as usize >= state.rounds.len() {
            return Err(CustomError::InvalidRound.into());
        }

        // Ensure the round is active and within the time limit
        let current_round = &mut state.rounds[round as usize];
        if !current_round.active || clock.unix_timestamp > current_round.end_time {
            return Err(CustomError::RoundExpired.into());
        }

        // Ensure the round has tokens available
        if current_round.balance == 0 {
            return Err(CustomError::InsufficientRoundBalance.into());
        }

        // Determine the price per token for the given round
        let price_per_token = current_round.price;

        // Calculate the number of tokens to transfer based on the price
        let one_token_amount = 10u64.pow(ctx.accounts.mint.decimals as u32); // Token decimals
        let one_payment_token_amount = 10u64.pow(ctx.accounts.payment_mint.decimals as u32); // Payment token decimals
        let receivable_amount = ((pay_amount as f64 / price_per_token)
            * (one_token_amount as f64 / one_payment_token_amount as f64))
            as u64;

        // Ensure there are enough tokens available in the round's balance
        if receivable_amount > current_round.balance {
            return Err(CustomError::InsufficientRoundBalance.into());
        }

        // Deduct tokens from the round's balance
        current_round.balance = current_round
            .balance
            .checked_sub(receivable_amount)
            .ok_or_else(|| CustomError::InsufficientRoundBalance)?;
        current_round.tokens_sold += receivable_amount;

        // Transfer payment tokens from buyer to admin
        let cpi_accounts_payment = Transfer {
            from: ctx.accounts.buyer_payment_mint_ata.to_account_info(),
            to: ctx.accounts.admin_ata.to_account_info(),
            authority: ctx.accounts.buyer.to_account_info(),
        };
        let cpi_program_payment = ctx.accounts.token_program.to_account_info();
        let cpi_ctx_payment = CpiContext::new(cpi_program_payment, cpi_accounts_payment);
        token::transfer(cpi_ctx_payment, pay_amount)?;

        // Create vesting account for buyer
        let vesting = &mut ctx.accounts.vesting;
        vesting.owner = ctx.accounts.buyer.key();
        vesting.purchases.push(Purchase {
            amount: receivable_amount,
            start_time: clock.unix_timestamp,
            round,
            claimed: false,
        });

        msg!(
            "Purchase completed. Buyer: {}. Amount: {}. Round: {}. Time: {}",
            buyer.key(),
            receivable_amount,
            round,
            clock.unix_timestamp
        );
        Ok(())
    }

    pub fn claim(ctx: Context<Claim>, round: u8, purchase_index: u64) -> Result<()> {
        let clock = clock::Clock::get().unwrap();
        let current_time = clock.unix_timestamp;
        let claimant = &ctx.accounts.claimant;
        let vesting = &mut ctx.accounts.vesting;

        // 5 minutes vesting period
        let five_mins_in_seconds = 5 * 60;

        // Ensure the purchase index is valid
        if purchase_index as usize >= vesting.purchases.len() {
            return Err(CustomError::InvalidPurchaseId.into());
        }

        // Get a mutable reference to the purchase
        let purchase = vesting.purchases.get_mut(purchase_index as usize).ok_or(CustomError::InvalidPurchaseId)?;

        require!(
            current_time >= purchase.start_time + five_mins_in_seconds,
            CustomError::VestingPeriodNotEnded
        );

        // Ensure the purchase has not already been claimed
        if purchase.claimed {
            return Err(CustomError::AlreadyClaimed.into());
        }

        let cpi_accounts = Transfer {
            from: ctx.accounts.pubsup_ata.to_account_info(),
            to: ctx.accounts.claimant_ata.to_account_info(),
            authority: ctx.accounts.pubsup_pda.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(
            CpiContext::new_with_signer(
                cpi_program,
                cpi_accounts,
                &[&[
                    PUB_POOL_SEEDS,
                    ctx.accounts.state.admin.as_ref(),
                    &[ctx.bumps["pubsup_pda"]],
                ]],
            ),
            purchase.amount,
        )?;
        // Mark the purchase as claimed
        purchase.claimed = true;

        msg!(
            "Claim successful. Claimant: {}. Amount: {}. Claimed: {}.",
            claimant.key(),
            purchase.amount,
            purchase.claimed
        );
        Ok(())
    }

    pub fn withdraw_remaining_tokens(ctx: Context<WithdrawRemainingTokens>) -> Result<()> {
        let state = &mut ctx.accounts.state;

        require!(
            ctx.accounts.admin.key() == state.admin,
            CustomError::Unauthorized
        );

        // Deactivate the last active round if round 2 is active
        if state.current_active_phase == 2 && state.rounds[2].active {
            let last_round_balance = state.rounds[2].balance;
            state.rounds[2].active = false;
            state.rounds[2].end_time = clock::Clock::get().unwrap().unix_timestamp;

            state.admin_remaining_tokens = state
                .admin_remaining_tokens
                .checked_add(last_round_balance)
                .unwrap();

            state.rounds[2].balance = 0;

            msg!("Round 2 deactivated and unsold tokens transferred to admin_remaining_tokens");
        }

        // Withdraw tokens from the public pool
        let pubsup_balance = ctx.accounts.pubsup_ata.amount;
        if pubsup_balance > 0 {
            let transfer_instruction = Transfer {
                from: ctx.accounts.pubsup_ata.to_account_info(),
                to: ctx.accounts.admin_ata.to_account_info(),
                authority: ctx.accounts.pubsup_pda.to_account_info(),
            };

            let pub_seeds = &[PUB_POOL_SEEDS.as_ref(), state.admin.as_ref(), &[ctx.bumps["pubsup_pda"]]];
            let pub_signer = &[&pub_seeds[..]];

            token::transfer(
                CpiContext::new_with_signer(ctx.accounts.token_program.to_account_info(), transfer_instruction, pub_signer),
                pubsup_balance,
            )?;

            state.pub_supply = 0;

            msg!("Public pool tokens withdrawn: {}", pubsup_balance);
        }

        // Withdraw tokens from the reserve pool
        let reserve_balance = ctx.accounts.reserve_ata.amount;
        if reserve_balance > 0 {
            let transfer_instruction = Transfer {
                from: ctx.accounts.reserve_ata.to_account_info(),
                to: ctx.accounts.admin_ata.to_account_info(),
                authority: ctx.accounts.reserve_pda.to_account_info(),
            };

            let reserve_seeds = &[RES_POOL_SEEDS.as_ref(), state.admin.as_ref(), &[ctx.bumps["reserve_pda"]]];
            let reserve_signer = &[&reserve_seeds[..]];

            token::transfer(
                CpiContext::new_with_signer(ctx.accounts.token_program.to_account_info(), transfer_instruction, reserve_signer),
                reserve_balance,
            )?;

            state.reserve_supply = 0;

            msg!("Reserve pool tokens withdrawn: {}", reserve_balance);
        }

        Ok(())
    }

    pub fn withdraw_public_pool_tokens(ctx: Context<WithdrawPublicPoolTokens>) -> Result<()> {
        let state = &mut ctx.accounts.state;

        require!(
            ctx.accounts.admin.key() == state.admin,
            CustomError::Unauthorized
        );
    
        // Withdraw tokens from the public pool
        let pubsup_balance = ctx.accounts.pubsup_ata.amount;
        if pubsup_balance > 0 {
            let transfer_instruction = Transfer {
                from: ctx.accounts.pubsup_ata.to_account_info(),
                to: ctx.accounts.admin_ata.to_account_info(),
                authority: ctx.accounts.pubsup_pda.to_account_info(),
            };
    
            let pub_seeds = &[PUB_POOL_SEEDS.as_ref(), state.admin.as_ref(), &[ctx.bumps["pubsup_pda"]]];
            let pub_signer = &[&pub_seeds[..]];
    
            token::transfer(
                CpiContext::new_with_signer(ctx.accounts.token_program.to_account_info(), transfer_instruction, pub_signer),
                pubsup_balance,
            )?;
    
            state.pub_supply = 0;
    
            msg!("Public pool tokens withdrawn: {}", pubsup_balance);
        } else {
            msg!("No tokens available in the public pool to withdraw.");
        }
    
        Ok(())
    }
    
    pub fn withdraw_reserve_pool_tokens(ctx: Context<WithdrawReservePoolTokens>) -> Result<()> {
        let state = &mut ctx.accounts.state;

        require!(
            ctx.accounts.admin.key() == state.admin,
            CustomError::Unauthorized
        );
    
        // Withdraw tokens from the reserve pool
        let reserve_balance = ctx.accounts.reserve_ata.amount;
        if reserve_balance > 0 {
            let transfer_instruction = Transfer {
                from: ctx.accounts.reserve_ata.to_account_info(),
                to: ctx.accounts.admin_ata.to_account_info(),
                authority: ctx.accounts.reserve_pda.to_account_info(),
            };
    
            let reserve_seeds = &[RES_POOL_SEEDS.as_ref(), state.admin.as_ref(), &[ctx.bumps["reserve_pda"]]];
            let reserve_signer = &[&reserve_seeds[..]];
    
            token::transfer(
                CpiContext::new_with_signer(ctx.accounts.token_program.to_account_info(), transfer_instruction, reserve_signer),
                reserve_balance,
            )?;
    
            state.reserve_supply = 0;
    
            msg!("Reserve pool tokens withdrawn: {}", reserve_balance);
        } else {
            msg!("No tokens available in the reserve pool to withdraw.");
        }
    
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    ///CHECK:
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account()]
    pub mint: Account<'info, Mint>,
    #[account()]
    pub payment_mint: Account<'info, Mint>,
    #[account(
        init,
        payer = admin,
        seeds = [b"state"],
        bump,
        space = 8 + std::mem::size_of::<State>()
    )]
    pub state: Box<Account<'info, State>>,
    ///CHECK:
    #[account(
        init,
        payer = admin,
        seeds = [PUB_POOL_SEEDS.as_ref(), admin.key().as_ref()],
        bump,
        space = 8 + std::mem::size_of::<Balance>()
    )]
    pub pubsup_pda: Account<'info, Balance>,
    ///CHECK:
    #[account(
        init,
        payer = admin,
        seeds = [RES_POOL_SEEDS.as_ref(), admin.key().as_ref()],
        bump,
        space = 8 + std::mem::size_of::<Balance>()
    )]
    pub reserve_pda: Account<'info, Balance>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, token::Token>,
}

#[derive(Accounts)]
pub struct Distoken<'info> {
    ///CHECK:
    #[account(mut)]
    pub admin: Signer<'info>,
    #[account(
        init_if_needed,
        payer = admin,
        associated_token::mint = mint,
        associated_token::authority = admin,
    )]
    pub admin_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub mint: Box<Account<'info, Mint>>,
    #[account(
        mut,
        seeds = [b"state"],
        bump,
    )]
    pub state: Box<Account<'info, State>>,
    ///CHECK:
    #[account(
        mut,
        seeds = [PUB_POOL_SEEDS.as_ref(), admin.key().as_ref()],
        bump,
    )]
    pub pubsup_pda: Box<Account<'info, Balance>>,
    #[account(
        init_if_needed,
        payer = admin,
        associated_token::mint = mint,
        associated_token::authority = pubsup_pda,
    )]
    pub pubsup_ata: Box<Account<'info, TokenAccount>>,
    ///CHECK:
    #[account(
        mut,
        seeds = [RES_POOL_SEEDS.as_ref(), admin.key().as_ref()],
        bump,
    )]
    pub reserve_pda: Account<'info, Balance>,
    #[account(
        init_if_needed,
        payer = admin,
        associated_token::mint = mint,
        associated_token::authority = reserve_pda,
    )]
    pub reserve_pool_ata: Box<Account<'info, TokenAccount>>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, token::Token>,
}

#[derive(Accounts)]
#[instruction(pay_amount: u64, round: u8)]
pub struct Purchasenow<'info> {
    ///CHECK:
    #[account(mut)]
    pub admin: AccountInfo<'info>,
    #[account(
        init_if_needed,
        payer = buyer,
        associated_token::mint = payment_mint,
        associated_token::authority = admin,
    )]
    pub admin_ata: Account<'info, TokenAccount>,
    #[account(
        mut,
        seeds = [b"state"],
        bump,
    )]
    pub state: Box<Account<'info, State>>,
    #[account(mut)]
    pub mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub payment_mint: Box<Account<'info, Mint>>,
    #[account(mut)]
    pub buyer: Signer<'info>,
    #[account(
        init_if_needed,
        payer = buyer,
        associated_token::mint = payment_mint,
        associated_token::authority = buyer,
    )]
    pub buyer_payment_mint_ata: Account<'info, TokenAccount>,
    #[account(
        init_if_needed,
        payer = buyer,
        space = 8 + Vesting::MAX_SIZE,
        seeds = [VEST_SEED.as_ref(), [round].as_ref(), buyer.key().as_ref()],
        bump
    )]
    pub vesting: Account<'info, Vesting>,
    pub token_program: Program<'info, token::Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(Accounts)]
#[instruction(_round: u8, purchase_index: u64)]
pub struct Claim<'info> {
    ///CHECK:
    #[account(mut)]
    pub claimant: Signer<'info>,
    #[account(
        mut,
        seeds = [b"state"],
        bump,
    )]
    pub state: Box<Account<'info, State>>,
    ///CHECK:
    #[account(
        mut,
        seeds = [PUB_POOL_SEEDS.as_ref(), state.admin.as_ref()],
        bump,
    )]
    pub pubsup_pda: Box<Account<'info, Balance>>,
    #[account(
        init_if_needed,
        payer = claimant,
        associated_token::mint = mint,
        associated_token::authority = pubsup_pda,
    )]
    pub pubsup_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub mint: Box<Account<'info, Mint>>,
    ///CHECK:
    #[account(
        mut,
        seeds = [VEST_SEED.as_ref(), [_round].as_ref(), claimant.key().as_ref()],
        bump
    )]
    pub vesting: Box<Account<'info, Vesting>>,
    #[account(
        init_if_needed,
        payer = claimant,
        associated_token::mint = mint,
        associated_token::authority = claimant,
    )]
    pub claimant_ata: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, token::Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(Accounts)]
pub struct StartRound<'info> {
    ///CHECK:
    #[account(mut, address = state.admin)]
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"state"],
        bump,
    )]
    pub state: Box<Account<'info, State>>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct WithdrawRemainingTokens<'info> {
    ///CHECK:
    #[account(mut, address = state.admin)]
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"state"],
        bump,
    )]
    pub state: Box<Account<'info, State>>,
    #[account(
        mut,
        seeds = [PUB_POOL_SEEDS.as_ref(), state.admin.as_ref()],
        bump,
    )]
    pub pubsup_pda: Box<Account<'info, Balance>>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = pubsup_pda,
    )]
    pub pubsup_ata: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        seeds = [RES_POOL_SEEDS.as_ref(), state.admin.as_ref()],
        bump,
    )]
    pub reserve_pda: Box<Account<'info, Balance>>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = reserve_pda,
    )]
    pub reserve_ata: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = admin,
    )]
    pub admin_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub mint: Box<Account<'info, Mint>>,
    pub token_program: Program<'info, token::Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(Accounts)]
pub struct WithdrawPublicPoolTokens<'info> {
    ///CHECK:
    #[account(mut, address = state.admin)]
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"state"],
        bump,
    )]
    pub state: Box<Account<'info, State>>,
    #[account(
        mut,
        seeds = [PUB_POOL_SEEDS.as_ref(), state.admin.as_ref()],
        bump,
    )]
    pub pubsup_pda: Box<Account<'info, Balance>>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = pubsup_pda,
    )]
    pub pubsup_ata: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = admin,
    )]
    pub admin_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub mint: Box<Account<'info, Mint>>,
    pub token_program: Program<'info, token::Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(Accounts)]
pub struct WithdrawReservePoolTokens<'info> {
    ///CHECK:
    #[account(mut, address = state.admin)]
    pub admin: Signer<'info>,
    #[account(
        mut,
        seeds = [b"state"],
        bump,
    )]
    pub state: Box<Account<'info, State>>,
    #[account(
        mut,
        seeds = [RES_POOL_SEEDS.as_ref(), state.admin.as_ref()],
        bump,
    )]
    pub reserve_pda: Box<Account<'info, Balance>>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = reserve_pda,
    )]
    pub reserve_ata: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = admin,
    )]
    pub admin_ata: Box<Account<'info, TokenAccount>>,
    #[account(mut)]
    pub mint: Box<Account<'info, Mint>>,
    pub token_program: Program<'info, token::Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}


#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct Round {
    pub active: bool,
    pub price: f64,
    pub balance: u64,
    pub tokens_sold: u64,
    pub start_time: i64,
    pub end_time: i64,
}

#[account]
pub struct State {
    pub admin: Pubkey,
    pub payment_mint: Pubkey,
    pub mint: Pubkey,
    pub pub_supply: u64,
    pub reserve_supply: u64,
    pub rounds: [Round; 3],
    pub current_active_phase: u8,
    pub admin_remaining_tokens: u64,
}

#[account]
pub struct Vesting {
    pub owner: Pubkey,
    pub purchases: Vec<Purchase>,
}

impl Vesting {
    pub const MAX_SIZE: usize = 8 + 32 + (8 + 8 + 1) * 100;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct Purchase {
    pub amount: u64,
    pub start_time: i64,
    pub round: u8,
    pub claimed: bool,
}

#[account]
pub struct Balance {
    pub balance: u64,
}

#[error_code]
pub enum CustomError {
    #[msg("The round has expired.")]
    RoundExpired,
    #[msg("The vesting period has not ended yet.")]
    VestingPeriodNotEnded,
    #[msg("Insufficient funds.")]
    InsufficientFunds,
    #[msg("Insufficient balance in the round.")]
    InsufficientRoundBalance,
    #[msg("Rounds must be started in increasing order.")]
    InvalidRoundOrder,
    #[msg("The specified round is invalid.")]
    InvalidRound,
    #[msg("Invalid purchase ID")]
    InvalidPurchaseId,
    #[msg("Already claimed")]
    AlreadyClaimed,
    #[msg("Only admin can perform this action.")]
    Unauthorized,
}