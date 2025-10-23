
#![allow(clippy::result_large_err)]
// mod cookbook;
mod util;

use anchor_lang::{prelude::*};
use anchor_spl::{associated_token::AssociatedToken, token_interface::{Mint, TokenAccount, TokenInterface, TransferChecked,transfer_checked}};  // self

use crate::util::DISCRIMITATOR_LEN;


declare_id!("7vbHJAeD8Akb6PBDGGpEoxH3WAZM6F12mUJAZqSmcZhE");

#[program]
pub mod unlock {

    use anchor_spl::token_interface;

    use super::*;

     pub fn create_vesting_account(ctx:Context<CreateVestingAccount>, company_name: String)  -> Result<()> {
       // 其实  CreateVestingAccount 中  其他的  都可以  自己初始化 ， 只有  vestingAccount 需要构造。 所以这里要构造
        *ctx.accounts.vesting_account = VestingAccount{
            owner: *ctx.accounts.signer.key,
            mint: ctx.accounts.mint.key(),
            treasury_token_account: ctx.accounts.treasury_token_account.key(),
            company_name,
            treasury_bump: ctx.bumps.treasury_token_account,
            bump: ctx.bumps.vesting_account,    // 但这里vesting_account 还没法有构造 好呀。 没有内容吧， 自己引自己的东西， 自己又没构造 好。 这个其实了解到， 只是seed， 计算好了的， 不用再重复计算
        };

        Ok(())
     }

     pub fn create_employee_account(ctx:Context<CreateEmployeeAccount>, start_time: i64, end_time: i64, cliff_time: i64, total_amount: u64 )  -> Result<()> {
        *ctx.accounts.employee_account = EmployeeAccount { 
            beneficiary: ctx.accounts.beneficiary.key(),
            start_time,
            end_time,
            cliff_time,
            total_amount,
            withdraw_amount: 0,
            bump: ctx.bumps.employee_account,
            vesting_account: ctx.accounts.vesting_account.key(),
            };
       
         Ok(())
      }

      // 在规定的时间 可以claim  token
      pub fn claim_tokens(ctx:Context<ClaimTokens> , _company_name: String)  -> Result<()> {
        // 为什么这里是  引用可变呢 ， 作者讲了 解引用 与它的区别
        // here we're goting to do a mutable reference.  That is because we need to specify   that the referenced account is going to change information.
        //  也就是对这个 employee_account  写， 引用 的内容改变了， 并不会回到原始数据呀， 那不是要后面再  赋值回来一次？ 
 

        // 那它与解引用 的区别是什么， 下面要讲  
        // so dereferencing is used when you need to manipulate the actual data stored in the reference location,   但引用 其实是一种浅拷贝吧， 引用的位置 ，是指向堆的指针吧， 这解一下， 就回到了原始位置 吧
        // it's kind of less common at a high level operations. It mainly used when you're initializing your resetting account data. 比较少见， 一般是初始化重置账户数据时使用。
        // earlier we were initializing two accounts 之前我们初始化2个账户， so we're using the deference operator  所以我们用了解引用 
        // but here we're borrowing mutable reference ,and this allows you to pass employee accounts around in your function or to other functions 
        //             while still having the ability to modify the original data.
        // 但在这里我们借用可变引用  , 这使你能够 在函数或其他函数中传递员工账户的同时 仍然能够 修改原始数据, 这在对数据进行多次操作时非常用有的。
        //and this is usefull when  you're using multiple operations on the data, which we're going to be doing as we're calculating to be able to claim tokens
        let employee_account = &mut ctx.accounts.employee_account;  
         
        let now  = Clock::get()?.unix_timestamp;
        if now < employee_account.cliff_time {
            return Err(VestingErrorCode::ClaimNotAvaliableYet.into())
        }

        // 接下来就是要做计算了， 计算前要做一些检查 ， 要考虑 underflow(下溢) 和 overflow (溢出)
        // 下溢是一个数字 它小于 对应数据类型的最小值，   saturate  使饱合， 浸透，  这种饱合减法， 保以保证 不会低于 0， 即防止下溢操作 
        let time_since_start = now.saturating_sub(employee_account.start_time);
        let total_vesting_time = employee_account.end_time.saturating_sub(employee_account.start_time);   
        if total_vesting_time  == 0 {  // 这个时间检查 ， 我会在 创建时检查 ，   如果结束的大于开始的 就报错， 账户都不让建。 
            return Err(VestingErrorCode::InvalidvestingPeriod.into())
        }

        let vested_amount = if now > employee_account.end_time{
            employee_account.total_amount
        }else{
            // checked_multiply  允许处理的溢出错误， 它的结果是个什么呢？
          match employee_account.total_amount.checked_mul(time_since_start as u64) {
            Some(product) => {
                product / total_vesting_time  as u64
            },
            None =>{ // 如果 不是error 而是一个panic ， 就会返回none的
                return Err(VestingErrorCode::CalculationOverflow.into())

            }
          }  
        };

        let claimable_amount =  vested_amount.saturating_sub(employee_account.withdraw_amount);

        if claimable_amount == 0 {
            return Err(VestingErrorCode::NothingToClaim.into())
        }
   
        // 调用 Cpi  去调转账， CPI 是跨程序转账的意思 ， 转账是 系统接口 ， 我程序 调系统程序， 所以是跨了程序。 有点像微服务中的rpc 调用 
        let transfer_cpi_accounts = TransferChecked {
            from: ctx.accounts.treasury_token_account.to_account_info(),
            mint: ctx.accounts.mint.to_account_info(),
            to: ctx.accounts.employee_token_account.to_account_info(),
            authority: ctx.accounts.treasury_token_account.to_account_info(),
        };

        let cpi_program = ctx.accounts.token_program.to_account_info();

        // 要获得可以签名的 财务 账户， 获得它的seed  以便推导  seed的类型很特别  3个引用  一个u8
        // 参照 treasury_token_account  的seed,   [b"vesting_treasury", company_name.as_bytes()], 比它多加一个自己的bump， 为什么这么做， 还不知道。
        let signer_seeds :  &[&[&[u8]]] = &[
          &[
             b"vesting_treasury",
             ctx.accounts.vesting_account.company_name.as_ref(),
             &[ctx.accounts.vesting_account.treasury_bump]
          ]
        ];

        let cpi_contex = CpiContext::new(cpi_program, transfer_cpi_accounts).with_signer(signer_seeds);
        let decimals = ctx.accounts.mint.decimals;
        transfer_checked(cpi_contex, claimable_amount as u64, decimals)?;


        // update the account state
        employee_account.withdraw_amount +=  claimable_amount;
        
        Ok(())
      }


    
}


#[derive(Accounts)]
#[instruction(company_name: String)]
pub struct ClaimTokens<'info> {
    #[account(mut)]
    pub beneficiary: Signer<'info>,

    #[account(  
        mut,
        seeds = [b"employee_account", beneficiary.key().as_ref(), vesting_account.key().as_ref()],
        bump = employee_account.bump  ,// 这个bump 是怎么来的，传来的吗？  自己的bump  == 自己的bump ， 必然成立， 等于没写
        has_one = vesting_account, // employee_account  中的 vesting_account 就是下面的vesting_account. 而且  vesting_account用了2次， 要保证能传递
        has_one = beneficiary,  // 约束 这个employee_account的 beneficiary 就是上面的beneficiary
    )]
    pub employee_account: Account<'info, EmployeeAccount>,

    #[account(
        mut,
        seeds = [company_name.as_ref()],
        has_one = treasury_token_account,  // 保证有 财务账号， 这才能从财务账号中转钱到这个 employ account
        has_one = mint,
        bump = vesting_account.bump,
    )]
    pub vesting_account: Account<'info, VestingAccount>,


    pub mint: InterfaceAccount<'info,Mint>,

    #[account(mut)]  // 因为要从它里面转走token， 所以要允许 写
    pub treasury_token_account: InterfaceAccount<'info,TokenAccount>,

    #[account(
        init_if_needed,  // 入账的账户， 它是一个tokenAccount。 有可能claim的时候是没有的， 没有就要建一个， 
        payer = beneficiary,
        associated_token::mint = mint,
        associated_token::authority = beneficiary,
        associated_token::token_program = token_program,
        
    )]
    pub employee_token_account: InterfaceAccount<'info,TokenAccount>  , 
    
    pub token_program: Interface<'info,TokenInterface>,
    pub associated_token_program: Program<'info,AssociatedToken>,
 
     pub system_program: Program<'info,System>,
}


#[derive(Accounts)]
#[instruction(company_name:String)]
pub struct CreateVestingAccount<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        init,
        payer = signer,
        space = DISCRIMITATOR_LEN + VestingAccount::INIT_SPACE,
        seeds = [company_name.as_ref()],
        bump,
    )]
    pub vesting_account: Account<'info,VestingAccount>,

    pub mint: InterfaceAccount<'info,Mint>,

    // 这个是专业为解锁 合约 指定的账户  just for the vesting contract , 所以pda 要简单， 以利于推导
    #[account(
        init,
        token::mint = mint,
        token::authority = treasury_token_account,  // 财务人员的账户，让他有权限给员工分派  token
        payer= signer,
        seeds = [b"vesting_treasury", company_name.as_bytes()],
        bump,

    )]
    pub treasury_token_account: InterfaceAccount<'info,TokenAccount>, // 财务货币账户， 便于分发token 用的

    pub system_program: Program<'info,System>,
    pub token_program: Interface<'info, TokenInterface>,


}

#[account]
#[derive(InitSpace)]
pub struct VestingAccount {
    pub owner: Pubkey,
    pub mint: Pubkey,
    pub treasury_token_account: Pubkey,
    #[max_len(50)]  // 长度超过， 会怎样？
    pub company_name: String,
    pub treasury_bump: u8,
    pub bump: u8,
}


// 感觉 除了 最后一个 withdraw amount 可以默认为 0, 其他的感觉都要传入
#[derive(Accounts)]
//#[instruction(employee_name:String)]
pub struct CreateEmployeeAccount<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,  // 谁创建谁签名， 一般就是财务人员 或老板, 谁就拥有这账户的所有权

    pub beneficiary: SystemAccount<'info>,  // 受益人不是普通的 账户吗， 怎么会是系统账户？  普通账户如何表示呢？
    
    #[account(
        has_one = owner  // 表明约束，vesting_account 是owner的。 这也保证 了  签名者有正确的访问权限 去运行本指令
    )]
    pub vesting_account: Account<'info,VestingAccount>,

     
    #[account(
        init,
        payer= owner,
        space = DISCRIMITATOR_LEN + EmployeeAccount::INIT_SPACE,
        seeds = [b"employee_account", beneficiary.key().as_ref(), vesting_account.key().as_ref()],
        bump,

    )]
    pub employee_account: Account<'info,EmployeeAccount>,   

    pub system_program: Program<'info,System>,
}


// 员工也要有账户， 存自己的内容
// 其实一个员工 可能会有多个token， 这里假设只有一种token，   总量， cliff 时期， start -end . 每隔多久  解锁多少token
// 这里没有唯一标识， 用beneficiary吗， 这个怎么做seed 
#[account]
#[derive(InitSpace)]
pub struct EmployeeAccount{

    pub beneficiary: Pubkey, // 意味着  员工要有一个自己的钱包  才能创建 ， siger 不要是员工 ， 而是公司建的， 最好能支持建一批的， 免点费用 

    // tracking start and end 
    pub start_time: i64,
    pub end_time: i64,
    pub cliff_time: i64,
    pub vesting_account: Pubkey, // 我认为直接 VestingAccount  会更合适。， 可能是传的东西更多， 导致存储很大吧
    pub total_amount: u64,
    pub withdraw_amount: u64,
    pub bump: u8,
}

#[error_code]
pub enum VestingErrorCode{

    #[msg("claim not available yet")]
    ClaimNotAvaliableYet,
    #[msg("invalid vesting period")]
    InvalidvestingPeriod,
    #[msg("calculation overflow")]
    CalculationOverflow,

    #[msg("nothing to claim")]
    NothingToClaim,

}

