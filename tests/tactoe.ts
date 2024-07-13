import * as anchor from "@coral-xyz/anchor";
import { BN, Program, web3 } from "@coral-xyz/anchor";
import { Tactoe } from "../target/types/tactoe";
import {
  MINT_SIZE,
  TOKEN_PROGRAM_ID,
  createAssociatedTokenAccountInstruction,
  createInitializeMintInstruction,
  createMintToInstruction,
  getAccount,
  getAssociatedTokenAddress,
  getMinimumBalanceForRentExemptMint,
} from "@solana/spl-token";
import { SystemProgram } from "@solana/web3.js";
import { assert } from "chai";

// 传入一个数字，生成一个0-num的随机数
const getRandom = (num: number) => {
  return Math.floor(Math.random() * (num + 1));
};

describe("tactoe", () => {
  const provider = anchor.AnchorProvider.local();
  // Configure the client to use the local cluster.
  anchor.setProvider(anchor.AnchorProvider.env());
  const program = anchor.workspace.Tactoe as Program<Tactoe>;
  const pg = program.provider;
  const wallet = provider.wallet;

  const gameKeypair = web3.Keypair.generate();
  it("Is setupGame!", async () => {
    const tx = await program.methods
      .setupGame()
      .accounts({
        game: gameKeypair.publicKey,
        playerOne: program.provider.publicKey,
        systemProgram: web3.SystemProgram.programId,
      })
      .signers([gameKeypair])
      .rpc();

    await pg.connection.confirmTransaction(tx);
  });

  it("Play", async () => {
    for (let i = 0; i < 10; i++) {
      const gameInfo = await program.account.game.fetch(gameKeypair.publicKey);
      if (gameInfo.state.active) {
        const balance = gameInfo.myBalance;
        const bidAmount = getRandom(balance);
        const tx = await program.methods
          .play(bidAmount)
          .accounts({
            game: gameKeypair.publicKey,
            player: program.provider.publicKey,
          })
          .signers([])
          .rpc();

        console.log(`bid ${i + 1} times , bidAmount: ${bidAmount}`);
        await pg.connection.confirmTransaction(tx);
      } else if (gameInfo.state.won) {
        console.log("You win!");
        break;
      } else if (gameInfo.state.lose) {
        console.log("You lost!");
        break;
      } else {
        console.log("Game is over!");
        break;
      }
    }
  });
  it("Transfers Spl Tokens to PDA", async () => {
    const lamports = await getMinimumBalanceForRentExemptMint(
      provider.connection
    );

    const [pda] = web3.PublicKey.findProgramAddressSync(
      [Buffer.from("vault"), wallet.publicKey.toBuffer()],
      program.programId
    );

    const tx = new anchor.web3.Transaction();
    const mintKp = anchor.web3.Keypair.generate();

    const walletAta = await getAssociatedTokenAddress(
      mintKp.publicKey,
      provider.wallet.publicKey
    );

    const pdaAta = await getAssociatedTokenAddress(mintKp.publicKey, pda, true);

    tx.add(
      SystemProgram.createAccount({
        fromPubkey: provider.wallet.publicKey,
        newAccountPubkey: mintKp.publicKey,
        space: MINT_SIZE,
        lamports: lamports,
        programId: TOKEN_PROGRAM_ID,
      }),

      createInitializeMintInstruction(
        mintKp.publicKey,
        9,
        provider.wallet.publicKey,
        provider.wallet.publicKey
      ),
      createAssociatedTokenAccountInstruction(
        provider.wallet.publicKey,
        walletAta,
        provider.wallet.publicKey,
        mintKp.publicKey
      ),
      createAssociatedTokenAccountInstruction(
        provider.wallet.publicKey,
        pdaAta,
        pda,
        mintKp.publicKey
      ),

      createMintToInstruction(
        mintKp.publicKey,
        pdaAta,
        provider.wallet.publicKey,
        1000
      )
    );

    await provider.sendAndConfirm(tx, [mintKp]);

    const txHash = await program.methods
      .getReward()
      .accounts({
        game: gameKeypair.publicKey,
        pda,
        pdaAta,
        toAta: walletAta,
        player: wallet.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .rpc();
    await pg.connection.confirmTransaction(txHash);

    const walletInfo = await getAccount(provider.connection, walletAta);
    const pdaInfo = await getAccount(provider.connection, pdaAta);

    console.log("walletInfo", walletInfo.amount.toString());
    console.log("pdaInfo", pdaInfo.amount.toString());

    assert.strictEqual(walletInfo.amount.toString(), "100");
    assert.strictEqual(pdaInfo.amount.toString(), "900");
  });
});
