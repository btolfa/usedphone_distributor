import * as anchor from "@coral-xyz/anchor";
import { PublicKey, Keypair } from '@solana/web3.js';
import {Program, web3, BN, AnchorProvider} from "@coral-xyz/anchor";

export async function createMintIfRequired(
  splProgram: Program,
  mint: Keypair,
  mintAuthority: PublicKey) {
  const mintAccount = await splProgram.account.mint.fetchNullable(mint.publicKey);
  if (mintAccount == null) {
    await splProgram.methods
      .initializeMint(9, mintAuthority, null)
      .accounts({
        mint: mint.publicKey,
        rent: web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([mint])
      .preInstructions([await splProgram.account.mint.createInstruction(mint)])
      .rpc();
  }
}

export async function createToken(
  splProgram: Program,
  token: Keypair,
  mint: PublicKey,
  owner: PublicKey
) {
  await splProgram.methods.initializeAccount()
    .accounts({
      account: token.publicKey,
      mint,
      owner,
      rent: web3.SYSVAR_RENT_PUBKEY,
    })
    .signers([token])
    .preInstructions([await splProgram.account.account.createInstruction(token)])
    .rpc();
}

export async function mintTo(
  spl_program: Program,
  amount: BN,
  mint: PublicKey,
  account: PublicKey,
  owner: PublicKey,
) {
  await spl_program.methods.mintTo(amount)
    .accounts({
      mint,
      account,
      owner,
    })
    .rpc();
}

export function getATA(owner: PublicKey, mint: PublicKey) {
  const [ata, _nonce] = PublicKey.findProgramAddressSync(
    [owner.toBuffer(), anchor.utils.token.TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    anchor.utils.token.ASSOCIATED_PROGRAM_ID
  );
  return ata;
}