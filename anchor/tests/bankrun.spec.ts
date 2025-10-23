import * as anchor from "@coral-xyz/anchor";
import {Program,BN} from "@coral-xyz/anchor";
import { Keypair, LAMPORTS_PER_SOL, PublicKey } from '@solana/web3.js';
import { BankrunProvider } from 'anchor-bankrun';
import { BanksClient, Clock, ProgramTestContext,startAnchor } from 'solana-bankrun'
//import IDL from "./features/idl/vesting.json";
import IDL from "../target/idl/unlock.json";
// import {Vesting} from './features/vesting';
import {Unlock as Vesting} from '../target/types/unlock';
import { SYSTEM_PROGRAM_ID } from "@coral-xyz/anchor/dist/cjs/native/system";
import {createMint, mintTo} from "spl-token-bankrun";
import {Key} from 'react';
import NodeWallet from "@coral-xyz/anchor/dist/cjs/nodewallet";
import { TOKEN_PROGRAM_ID } from "@coral-xyz/anchor/dist/cjs/utils/token";
import { resolve } from "path";
// import { CommitmentLevel } from "solana-bankrun/dist/internal";

// 在做这些测试的时候 并不需要开启  solana-test-validator
describe("vesting smart contract test",() =>{

    let companyName = "abc company";
    let beneficiary: Keypair;
    let context: ProgramTestContext;
    let provider: BankrunProvider;
    let program: Program<Vesting>;
    let beneficiaryProgram: Program<Vesting>;
    let banksClient: BanksClient;
    let employer: Keypair;
    let mint : PublicKey;
    let beneficiaryProvider: BankrunProvider;
    let vestingAccountKey: PublicKey;
    let treasuryTokenAccountKey:PublicKey;
    let employeeAccount: PublicKey;

     beforeAll(async () => {
        // 测试前的准备，
        // 1 一个已有资产的钱包，
        // 有2 个测试案例， employee  和  employer 视角
        // 使用  环境中的钱包， 作 employer 钱包
        beneficiary = new anchor.web3.Keypair();

        context = await startAnchor(
            "",
            [{name:"vesting", programId: new PublicKey(IDL.address)}],
            [{
                address: beneficiary.publicKey,
                info:{
                    lamports: 1_000_000_000,
                    data: new Uint8Array(Buffer.alloc(0)),
                    owner: SYSTEM_PROGRAM_ID,
                    executable: false,
                },
            }]
        );


        provider = new  BankrunProvider(context);
        anchor.setProvider(provider);

        program = new Program<Vesting>(IDL as Vesting,provider);

        banksClient = context.banksClient;
        employer =provider.wallet.payer;


        // 老师的那会有 类型提示错误  banksclient  . 类型的错误 ， 是因为  , spl-token-bankrun 和  anchor-bankrun 之间有区别， 但也有联系， 它们之间有依赖关系的
        // 可以的做法是用 TS ignore 去忽略掉   写法     
        // // @ts-expect-error - Type error in spl-token-bankrun dependency
        mint = await createMint(banksClient,employer, employer.publicKey, null,2 ); // null 是frozenAuthority , 老师的  banksclient 参数会报错

        beneficiaryProvider = new BankrunProvider(context);
        beneficiaryProvider.wallet = new NodeWallet(beneficiary);

        beneficiaryProgram = new Program<Vesting>(IDL as Vesting,beneficiaryProvider);

        [vestingAccountKey] = PublicKey.findProgramAddressSync(   // 没有实例化， 可以直接找到的嘛？
            [Buffer.from(companyName)],
            program.programId
        );

        [treasuryTokenAccountKey] = PublicKey.findProgramAddressSync(
            [Buffer.from("vesting_treasury"), Buffer.from(companyName)],
            program.programId,
        );

        [employeeAccount] = PublicKey.findProgramAddressSync(
            [Buffer.from("employee"), beneficiary.publicKey.toBuffer(),vestingAccountKey.toBuffer()],
            program.programId,
        );

     });

     it("should create a vesting account", async () =>{
        // 定义所有 需要的账户， 传入参数， 如vestingAccount (这个不就是要建的吧)， treasuryTokenAccount， mint, tokenProgram
        const tx = await program.methods.createVestingAccount(companyName).accounts({
            signer: employer.publicKey,
            mint,
            tokenProgram: TOKEN_PROGRAM_ID,
        }).rpc({commitment: 'confirmed'});

        const vestingAccountData = await program.account.vestingAccount.fetch(vestingAccountKey,'confirmed');
        console.log('vesting account data: ', vestingAccountData, null,2); // null , 2 是什么意思 ？
        console.log('create vesting account : ' ,tx);
     })



     it("should fund the treasury token account", async () =>{
        const amount = 10* 10**9;
        const mintTx = await mintTo(
            banksClient,
            employer,
            mint,
            treasuryTokenAccountKey,
            employer,
            amount,
        );

        console.log("mint treasury token account : ", mintTx);
     });

   
     it(" should create  employee vesting token account",async ()=>{
         const tx2 = program.methods
         .createEmployeeAccount(new BN(0), new BN(100), new BN(0), new BN(100))
         .accounts({
            beneficiary: beneficiary.publicKey,
            vestingAccount: vestingAccountKey,
         }).rpc({commitment: "confirmed", skipPreflight: true});

         console.log("create employee account tx:" , tx2);
         console.log("Employee Account: ", employeeAccount.toBase58() ); // 能看到账户有token?

         
     });

     it("should claim the employee's vested token", async () =>{
       await new Promise((a) => setTimeout(a,1000) ); // 设置超时时间  
       const currentClock = await banksClient.getClock();


       context.setClock(
        new Clock(
            currentClock.slot,
            currentClock.epochStartTimestamp,
            currentClock.epoch,
            currentClock.leaderScheduleEpoch,
            1000n
                )
       );

       const tx3 = program.methods.claimTokens(companyName)
       .accounts({tokenProgram: TOKEN_PROGRAM_ID})
       .rpc({commitment: "confirmed"});

       console.log("claim tokens  tx", tx3);


     });



});
