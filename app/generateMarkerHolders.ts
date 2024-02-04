import * as anchor from '@coral-xyz/anchor';

import {Keypair, PublicKey} from "@solana/web3.js";
import { readFileSync } from "fs";
import {BN, Program, web3} from "@coral-xyz/anchor";
import {SPL_TOKEN_PROGRAM_ID, splTokenProgram} from "@coral-xyz/spl-token";
import {createAssociatedTokenAccountInstruction} from "@solana/spl-token";
import fs from "fs";

const MARKER_MINT = new PublicKey("9Dysc3vtrrYr7eaFX3gosGfyJgKxzQmZs47pmhWMG1P");

function getATA(owner: PublicKey, mint: PublicKey): PublicKey {
  const [ata, _nonce] = PublicKey.findProgramAddressSync(
    [owner.toBuffer(), SPL_TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    anchor.utils.token.ASSOCIATED_PROGRAM_ID
  );
  return ata;
}

async function mintToATA(
  spl_program: Program,
  payer: PublicKey,
  owner: PublicKey,
  amount: BN,
  mint: PublicKey,
  mintAuthority: Keypair
) {
  const ata = getATA(owner, mint);

  await spl_program.methods.mintTo(amount)
    .accounts({
      mint: mint,
      to: ata,
      authority: mintAuthority.publicKey,
    })
    .preInstructions([
      createAssociatedTokenAccountInstruction(
        payer,
        ata,
        owner,
        mint)
    ])
    .signers([mintAuthority])
    .rpc();

  return ata;
}

anchor.setProvider(anchor.AnchorProvider.env());

async function main() {
  const provider = anchor.AnchorProvider.env();
  const splProgram = splTokenProgram({
    provider,
    programId: SPL_TOKEN_PROGRAM_ID
  });
  const mintAuthority = Keypair.fromSecretKey(new Uint8Array(JSON.parse(fs.readFileSync('keys/authority.json').toString())));

  const wallet = anchor.Wallet.local();

  const amount = new BN(1000000000);

  for (let i = 0; i < 100; i++) {
    const receiver = Keypair.generate();

    const ata = getATA(receiver.publicKey, MARKER_MINT);

    const tx = await splProgram.methods.mintTo(amount)
      .accounts({
        mint: MARKER_MINT,
        account: ata,
        owner: mintAuthority.publicKey,
      })
      .preInstructions([
        createAssociatedTokenAccountInstruction(
          wallet.publicKey,
          ata,
          receiver.publicKey,
          MARKER_MINT)
      ])
      .signers([mintAuthority])
      .rpc();

    console.log("minted to ", ata.toString(), " with tx: ", tx);
  }
}

main();