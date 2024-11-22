import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { Sepawithdraw } from "../target/types/sepawithdraw";
import { getAssociatedTokenAddressSync, createAssociatedTokenAccountInstruction, TOKEN_PROGRAM_ID, ASSOCIATED_TOKEN_PROGRAM_ID, createTransferInstruction } from "@solana/spl-token";
import * as bs58 from "bs58";
import { assert } from "chai";
import { utf8 } from "@coral-xyz/anchor/dist/cjs/utils/bytes";

const log = console.log;

describe("sepawithdraw", () => {
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());
  const provider = anchor.AnchorProvider.env();
  const program = anchor.workspace.Sepawithdraw as Program<Sepawithdraw>;

  const admin = provider.publicKey;
  const STATE_SEEDS = utf8.encode("state");
  const PUB_POOL_SEEDS = utf8.encode("public_pool");
  const RES_POOL_SEEDS = utf8.encode("reserve_pool");
  const BUR_POOL_SEEDS = utf8.encode("burn_pool");
  const VEST_SEED = utf8.encode("vesting");
  const TOKEN_SUPPLY = 999_000_000_000_000;

  const [state_pda, state_bump] = anchor.web3.PublicKey.findProgramAddressSync([STATE_SEEDS], program.programId);
  const [pubsup_pda, pubsup_bump] = anchor.web3.PublicKey.findProgramAddressSync([PUB_POOL_SEEDS, admin.toBuffer()], program.programId);
  const [reserve_pool_pda, reserve_bump] = anchor.web3.PublicKey.findProgramAddressSync([RES_POOL_SEEDS, admin.toBuffer()], program.programId);

  const secretKey = bs58.decode("2WMkZCsM35kCzTyDqwGWZEpm92zsdRgvHkEiSFR3Rj4uLsNd9Gxd3bJddZcBt1BPZRXodWwSU82vFVGnmCwehxQf");
  // const secretKey = bs58.decode("4thJdTGEioqQbqLGqGn1dBWddpkzyMokJ2yWcqdHefTRBBfzuDFC3tndKoCGQLAjDQyZLXxEuwWSvhbZDseTR3Ha");

  const buyerKeypair = anchor.web3.Keypair.fromSecretKey(secretKey);
  log("Buyer:", buyerKeypair.publicKey.toBase58());

  const tokenMint = new anchor.web3.PublicKey("ALiUe3kWSkjRnjmEfqhpvXbWR6i32rYAZMxURDYLGJHf");
  const paymentMint = new anchor.web3.PublicKey("9U4NYasbyDSjQTJx1WzS5HtLcR2fKwY7jLAxHy1pZorC");

  const txis: anchor.web3.TransactionInstruction[] = [];

  async function getorcreateTokenAccount(
    token: anchor.web3.PublicKey,
    owner: anchor.web3.PublicKey,
    isOffCurve: boolean = false,
  ): Promise<anchor.web3.PublicKey> {
    const tokenAccount = getAssociatedTokenAddressSync(token, owner, isOffCurve);
    let accountinfo = await provider.connection.getAccountInfo(tokenAccount);
    if (accountinfo == null) {
      const txi = createAssociatedTokenAccountInstruction(provider.publicKey, tokenAccount, owner, token);
      txis.push(txi)
    }
    log("Token_Account", tokenAccount.toBase58());
    return tokenAccount;
  }

  // it("Is initialized!", async () => {
  //   const tx = await program.methods.initialize().accounts({
  //     admin: admin,
  //     mint: tokenMint,
  //     paymentMint: paymentMint,
  //     state: state_pda,
  //     pubsupPda: pubsup_pda,
  //     reservePda: reserve_pool_pda,
  //     systemProgram: anchor.web3.SystemProgram.programId,
  //     tokenProgram: TOKEN_PROGRAM_ID,
  //   }).instruction();
  //   txis.push(tx);
  //   const transaction = new anchor.web3.Transaction().add(...txis);
  //   const sign = await provider.sendAndConfirm(transaction)
  //     .catch((sendTxError) => {
  //       log({ sendTxError });
  //       throw "init main failed";
  //     });;
  //   log("Admin:", admin.toBase58());
  //   log("Your signature", sign);
  // });

  it("Distributes tokens to pools", async () => {
         const adminAta = await getorcreateTokenAccount(tokenMint, admin);
         const pubsupAta = await getorcreateTokenAccount(tokenMint, pubsup_pda, true);
         const reserveAta = await getorcreateTokenAccount(tokenMint, reserve_pool_pda, true);
  
         const tx = await program.methods.dogdistribution(new anchor.BN(TOKEN_SUPPLY)).accounts({
           admin: admin,
           adminAta: adminAta,
           mint: tokenMint,
           state: state_pda,
           pubsupPda: pubsup_pda,
           pubsupAta: pubsupAta,
           reservePda: reserve_pool_pda,
           reservePoolAta: reserveAta,
           systemProgram: anchor.web3.SystemProgram.programId,
           associatedTokenProgram: ASSOCIATED_TOKEN_PROGRAM_ID,
           tokenProgram: TOKEN_PROGRAM_ID,
           }).instruction();
           txis.push(tx);
           const transaction = new anchor.web3.Transaction().add(...txis);
           const sign = await provider
           .sendAndConfirm(transaction)
           .catch((distributeError) => {
           log({ distributeError });
           });
           log("Distribution complete. Signature:", sign);
  
           const state = await program.account.state.fetch(state_pda);
          //  assert.equal(state.pubSupply.toString(), "75000000000000", "Public supply should be set correctly");
          //  assert.equal(state.reserveSupply.toString(), "30000000000000", "Reserve supply should be set correctly");
          //  assert.equal(state.burnSupply.toString(), "45000000000000", "Burn supply should be set correctly");
  
           // Check round supplies
          //  assert.equal(state.rounds[0].balance.toString(), "45000000000000", "Round 1 supply should be set correctly");
          //  assert.equal(state.rounds[1].balance.toString(), "15000000000000", "Round 2 supply should be set correctly");
          //  assert.equal(state.rounds[2].balance.toString(), "15000000000000", "Round 3 supply should be set correctly");
     });
});
