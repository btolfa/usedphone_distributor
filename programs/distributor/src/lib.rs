pub mod error;

use anchor_lang::{prelude::*, system_program};
use anchor_spl::{
    associated_token::{self, get_associated_token_address_with_program_id, AssociatedToken, Create as CreateAta},
    token_interface::{self, Burn, Mint, TokenAccount, TokenInterface, TransferChecked},
};
use itertools::Itertools;

use error::DistributorError;

declare_id!("5YP6jdWGTNDUhLYMCfocbyfT4RN58QbhVdtYmBdL6Af1");

#[program]
pub mod distributor {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, share_size: u64, number_of_shares: u64) -> Result<()> {
        require_gt!(share_size, 0, DistributorError::InvalidParameters);
        require_gt!(number_of_shares, 1, DistributorError::InvalidParameters);
        require!(
            share_size.checked_mul(number_of_shares).is_some(),
            DistributorError::InvalidParameters
        );

        let distributor_state = &mut ctx.accounts.distributor_state;
        distributor_state.vault = ctx.accounts.vault.key();
        distributor_state.mint = ctx.accounts.mint.key();
        distributor_state.marker_mint = ctx.accounts.marker_mint.key();
        distributor_state.distributor_authority = ctx.accounts.distributor_authority.key();
        distributor_state.share_size = share_size;
        distributor_state.number_of_shares = number_of_shares;
        distributor_state.distributor_state_bump = ctx.bumps.distributor_state;
        distributor_state.vault_bump = ctx.bumps.vault;

        Ok(())
    }

    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        let decimals = ctx.accounts.mint.decimals;
        token_interface::transfer_checked(ctx.accounts.into(), amount, decimals)
    }

    pub fn distribute<'c: 'info, 'info>(ctx: Context<'_, '_, 'c, 'info, Distribute<'info>>) -> Result<()> {
        let vault_amount = ctx.accounts.vault.amount;
        let threshold = ctx.accounts.distributor_state.threshold();
        require_gte!(vault_amount, threshold, DistributorError::ThresholdNotMet);

        let number_of_shares = ctx.accounts.distributor_state.number_of_shares;
        let remaining_accounts = ctx.remaining_accounts;
        // There is have to be (number_of_shares - 1) * 2 accounts - authority and token account
        // for each share without last one
        require_eq!(
            remaining_accounts.len() as u64,
            (number_of_shares - 1) * 2,
            DistributorError::MissingRemainingAccounts
        );

        let mint = ctx.accounts.mint.key();
        let mint_marker = ctx.accounts.distributor_state.marker_mint;
        let share_size = ctx.accounts.distributor_state.share_size.to_le_bytes();
        let number_of_shares = ctx.accounts.distributor_state.number_of_shares.to_le_bytes();

        let seeds = [
            mint.as_ref(),
            mint_marker.as_ref(),
            share_size.as_ref(),
            number_of_shares.as_ref(),
            &[ctx.accounts.distributor_state.distributor_state_bump],
        ];

        let token_program = ctx.accounts.token_program.key();
        for (authority, token_account) in ctx.remaining_accounts.iter().tuples() {
            require_keys_eq!(
                *token_account.key,
                get_associated_token_address_with_program_id(authority.key, &mint, &token_program),
                DistributorError::InvalidAssociatedTokenAccount
            );

            // token account is not initialized
            if token_account.owner == &system_program::ID && token_account.lamports() == 0 {
                associated_token::create(CpiContext::new(
                    ctx.accounts.associated_token_program.to_account_info(),
                    CreateAta {
                        payer: ctx.accounts.payer.to_account_info(),
                        associated_token: token_account.to_account_info(),
                        authority: authority.to_account_info(),
                        mint: ctx.accounts.mint.to_account_info(),
                        system_program: ctx.accounts.system_program.to_account_info(),
                        token_program: ctx.accounts.token_program.to_account_info(),
                    },
                ))?;
            }
            let token_account = InterfaceAccount::<TokenAccount>::try_from(token_account)?;
            require_keys_eq!(
                token_account.mint,
                ctx.accounts.mint.key(),
                DistributorError::InvalidAssociatedTokenAccount
            );
            require_keys_eq!(
                token_account.owner,
                *authority.key,
                DistributorError::InvalidAssociatedTokenAccount
            );

            token_interface::transfer_checked(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    TransferChecked {
                        from: ctx.accounts.vault.to_account_info(),
                        mint: ctx.accounts.mint.to_account_info(),
                        to: token_account.to_account_info(),
                        authority: ctx.accounts.distributor_state.to_account_info(),
                    },
                    &[&seeds],
                ),
                ctx.accounts.distributor_state.share_size,
                ctx.accounts.mint.decimals,
            )?;
        }

        token_interface::burn(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Burn {
                    mint: ctx.accounts.mint.to_account_info(),
                    from: ctx.accounts.vault.to_account_info(),
                    authority: ctx.accounts.distributor_state.to_account_info(),
                },
                &[&seeds],
            ),
            ctx.accounts.distributor_state.share_size,
        )
    }
}

#[derive(Accounts)]
#[instruction(share_size: u64, number_of_shares: u64)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = 8 + DistributorState::INIT_SPACE,
        seeds = [
            mint.key().as_ref(),
            marker_mint.key().as_ref(),
            share_size.to_le_bytes().as_ref(),
            number_of_shares.to_le_bytes().as_ref()
        ],
        bump
    )]
    pub distributor_state: Account<'info, DistributorState>,

    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        init,
        payer = payer,
        seeds = [distributor_state.key().as_ref()],
        bump,
        token::mint = mint,
        token::authority = distributor_state,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    pub marker_mint: InterfaceAccount<'info, Mint>,

    /// CHECK: will be used only for key
    pub distributor_authority: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[account]
#[derive(InitSpace)]
pub struct DistributorState {
    pub vault: Pubkey,
    pub mint: Pubkey,
    pub marker_mint: Pubkey,
    pub distributor_authority: Pubkey,

    pub share_size: u64,
    pub number_of_shares: u64,

    pub distributor_state_bump: u8,
    pub vault_bump: u8,
}

impl DistributorState {
    pub fn threshold(&self) -> u64 {
        self.share_size * self.number_of_shares
    }
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(
        has_one = mint,
        has_one = vault,
        seeds = [
            mint.key().as_ref(),
            distributor_state.marker_mint.as_ref(),
            distributor_state.share_size.to_le_bytes().as_ref(),
            distributor_state.number_of_shares.to_le_bytes().as_ref()
        ],
        bump = distributor_state.distributor_state_bump
    )]
    pub distributor_state: Account<'info, DistributorState>,

    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        seeds = [distributor_state.key().as_ref()],
        bump = distributor_state.vault_bump,
        token::mint = mint,
        token::authority = distributor_state,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    pub authority: Signer<'info>,
    #[account(
        mut,
        token::mint = mint,
        token::authority = authority,
    )]
    pub token_account: InterfaceAccount<'info, TokenAccount>,

    pub token_program: Interface<'info, TokenInterface>,
}

impl<'a, 'b, 'c, 'info> From<&mut Deposit<'info>> for CpiContext<'a, 'b, 'c, 'info, TransferChecked<'info>> {
    fn from(accounts: &mut Deposit<'info>) -> CpiContext<'a, 'b, 'c, 'info, TransferChecked<'info>> {
        let cpi_accounts = TransferChecked {
            from: accounts.token_account.to_account_info(),
            mint: accounts.mint.to_account_info(),
            to: accounts.vault.to_account_info(),
            authority: accounts.authority.to_account_info(),
        };
        let cpi_program = accounts.token_program.to_account_info();
        CpiContext::new(cpi_program, cpi_accounts)
    }
}

#[derive(Accounts)]
pub struct Distribute<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    pub distributor_authority: Signer<'info>,

    #[account(
        has_one = distributor_authority,
        has_one = mint,
        has_one = vault,
        seeds = [
                mint.key().as_ref(),
                distributor_state.marker_mint.as_ref(),
                distributor_state.share_size.to_le_bytes().as_ref(),
                distributor_state.number_of_shares.to_le_bytes().as_ref()
        ],
        bump = distributor_state.distributor_state_bump
    )]
    pub distributor_state: Account<'info, DistributorState>,

    #[account(mut)]
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(
        mut,
        seeds = [distributor_state.key().as_ref()],
        bump = distributor_state.vault_bump,
        token::mint = mint,
        token::authority = distributor_state,
    )]
    pub vault: InterfaceAccount<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}
