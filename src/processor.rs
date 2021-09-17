use solana_program::{
    account_info::{next_account_info, AccountInfo},
    entrypoint::ProgramResult,
    program_error::ProgramError,
    msg,
    pubkey::Pubkey,
    program_pack::{Pack, IsInitialized},
    sysvar::{rent::Rent, Sysvar},
    program::{invoke, invoke_signed}
};

use spl_token::state::Account as TokenAccount;

use crate::{instruction::EscrowInstruction, error::EscrowError, state::Escrow};

pub struct Processor;
impl Processor {
    pub fn process(program_id: &Pubkey, accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {
        let instruction = EscrowInstruction::unpack(instruction_data)?;

        match instruction {
            EscrowInstruction::InitEscrow { amount } => {
                msg!("Instruction: InitEscrow");
                Self::process_init_escrow(accounts, amount, program_id)
            },
            EscrowInstruction::Exchange { amount } => {
                msg!("Instruction: Exchange");
                Self::process_exchange(accounts, amount, program_id)
            }
        }
    }

    fn process_exchange(
        accounts: &[AccountInfo],
        amount: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();

        let taker = next_account_info(account_info_iter)?;
        if !taker.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let taker_token_to_send_account = next_account_info(account_info_iter)?;

        let taker_token_to_receive_account = next_account_info(account_info_iter)?;

        let pda_temp_token_account = next_account_info(account_info_iter)?;
        let pda_temp_token_account_info = 
            TokenAccount::unpack(&pda_temp_token_account.data.borrow())?;
        if amount != pda_temp_token_account_info.amount {
            return Err(EscrowError::ExpectedAmountMismatch.into());
        }

        let initializer_account = next_account_info(account_info_iter)?;

        let initializer_token_to_receive_account = next_account_info(account_info_iter)?;

        let escrow_account = next_account_info(account_info_iter)?;
        let escrow_info = Escrow::unpack(&escrow_account.data.borrow())?;
        if escrow_info.temp_token_account_pubkey != *pda_temp_token_account.key {
            return Err(ProgramError::InvalidAccountData);
        }
        if escrow_info.initializer_pubkey != *initializer_account.key {
            return Err(ProgramError::InvalidAccountData);
        }
        if escrow_info.initializer_token_to_receive_account_pubkey != *initializer_token_to_receive_account.key {
            return Err(ProgramError::InvalidAccountData);
        }

        let token_program = next_account_info(account_info_iter)?;

        let pda_account = next_account_info(account_info_iter)?;

        // send ix to transfer token y to initializer
        let transfer_to_initializer_ix = spl_token::instruction::transfer(
            token_program.key,
            taker_token_to_send_account.key,
            initializer_token_to_receive_account.key,
            taker.key,
            &[&taker.key],
            escrow_info.expected_amount,
        )?;
        msg!("Calling the token program to transfer tokens to the escrow's initializer...");
        invoke(
            &transfer_to_initializer_ix,
            &[
                taker_token_to_send_account.clone(),
                initializer_token_to_receive_account.clone(),
                taker.clone(),
                token_program.clone(),
            ],
        )?;

        // send ix to transfer token x to taker        
        let (pda, bump_seed) = Pubkey::find_program_address(&[b"escrow"], program_id);

        let transfer_to_taker_ix = spl_token::instruction::transfer(
            token_program.key,
            pda_temp_token_account.key,
            taker_token_to_receive_account.key,
            &pda,
            &[&pda],
            pda_temp_token_account_info.amount,
        )?;
        msg!("Calling the token program to transfer tokens to the taker...");
        invoke_signed(
            &transfer_to_taker_ix,
            &[
                pda_temp_token_account.clone(),
                taker_token_to_receive_account.clone(),
                pda_account.clone(),
                token_program.clone(),
            ],
            &[&[&b"escrow"[..], &[bump_seed]]],
        )?;

        // close pda temp token account
        let close_pdas_temp_acc_ix = spl_token::instruction::close_account(
            token_program.key,
            pda_temp_token_account.key,
            initializer_account.key,
            &pda,
            &[&pda]
        )?;
        msg!("Calling the token program to close pda's temp account");
        invoke_signed(
            &close_pdas_temp_acc_ix,
            &[
                pda_temp_token_account.clone(),
                initializer_account.clone(),
                pda_account.clone(),
                token_program.clone(),
            ],
            &[&[&b"escrow"[..], &[bump_seed]]],
        )?;

        msg!("Closing the escrow account...");
        **initializer_account.lamports.borrow_mut() = initializer_account.lamports()
            .checked_add(escrow_account.lamports())
            .ok_or(EscrowError::AmountOverflow)?;
        **escrow_account.lamports.borrow_mut() = 0;
        *escrow_account.data.borrow_mut() = &mut []; // clearing the data is important

        Ok(())
    }

    fn process_init_escrow(
        accounts: &[AccountInfo],
        amount: u64,
        program_id: &Pubkey,
    ) -> ProgramResult {
        let account_info_iter = &mut accounts.iter();

        let initializer = next_account_info(account_info_iter)?;
        if !initializer.is_signer {
            return Err(ProgramError::MissingRequiredSignature);
        }

        let temp_token_account = next_account_info(account_info_iter)?;

        let token_to_receive_account = next_account_info(account_info_iter)?;
        if *token_to_receive_account.owner != spl_token::id() {
            return Err(ProgramError::IncorrectProgramId);
        }
        // verify this account is not a mint account

        let escrow_account = next_account_info(account_info_iter)?;
        
        let rent = &Rent::from_account_info(next_account_info(account_info_iter)?)?;
        if !rent.is_exempt(escrow_account.lamports(), escrow_account.data_len()) {
            return Err(EscrowError::NotRentExempt.into());
        }
        
        let mut escrow_info = Escrow::unpack_unchecked(&escrow_account.data.borrow())?;
        if escrow_info.is_initialized() {
            return Err(ProgramError::AccountAlreadyInitialized);
        }

        escrow_info.is_initialized = true;
        escrow_info.initializer_pubkey = *initializer.key;
        escrow_info.temp_token_account_pubkey = *temp_token_account.key;
        escrow_info.initializer_token_to_receive_account_pubkey = *token_to_receive_account.key;
        escrow_info.expected_amount = amount;

        Escrow::pack(escrow_info, &mut escrow_account.data.borrow_mut())?;

        // PDA aka program derived addresses - these do not lie on the ed25519 curve and have no private key associated
        let (pda, _bump_seed) = Pubkey::find_program_address(&[b"escrow"], program_id);
        let token_program = next_account_info(account_info_iter)?;
        let owner_change_ix = spl_token::instruction::set_authority(
            token_program.key, // token program id
            temp_token_account.key, // account whos authority we wanna change
            Some(&pda), // account that is going to be the new authority - our generated PDA
            spl_token::instruction::AuthorityType::AccountOwner, // type
            initializer.key, // current account owner
            &[&initializer.key], // public keys signing the CPI (cross program invocation)
        )?;    

        msg!("Calling the token program to transfer token account ownership...");
        invoke(
            &owner_change_ix,
            &[
                temp_token_account.clone(),
                initializer.clone(),
                token_program.clone(),
            ],
        )?;

        Ok(())
    }
}
