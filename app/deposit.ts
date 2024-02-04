import * as anchor from '@coral-xyz/anchor';

import { PublicKey } from "@solana/web3.js";
import { readFileSync } from "fs";
import { BN } from "@coral-xyz/anchor";

// Deployed program id
const PROGRAM_ID = new PublicKey("5YP6jdWGTNDUhLYMCfocbyfT4RN58QbhVdtYmBdL6Af1");
const DISTRIBUTOR_STATE = new PublicKey("EBHnjoKTCn4S27pYsfYesRbnVr3JmAHg6E5JEnrgAqCR");
const VAULT = new PublicKey("4wZ2E3St33iB5xu9R2Kf6NbMa5pkoeqVNe1SkcFVvoX5");
const MINT = new PublicKey("6VzNqK5a68KTqfkC2xRzF3fH5kKKAA8D2xdPCe6FDxS7");

function distributorProgram() {
  const idl = JSON.parse(
    readFileSync("./target/idl/distributor.json", "utf8")
  );

  return new anchor.Program(idl, PROGRAM_ID);
}

function getATA(owner: PublicKey, mint: PublicKey): PublicKey {
  const [ata, _nonce] = PublicKey.findProgramAddressSync(
    [owner.toBuffer(), anchor.utils.token.TOKEN_PROGRAM_ID.toBuffer(), mint.toBuffer()],
    anchor.utils.token.ASSOCIATED_PROGRAM_ID
  );
  return ata;
}

anchor.setProvider(anchor.AnchorProvider.env());

async function main() {
  const program = distributorProgram();
  const wallet = anchor.Wallet.local();

  let tokenAccount = getATA(wallet.publicKey, MINT);
  let amount = new BN(1000000000);

  await program.methods.deposit(amount).accounts({
    distributorState: DISTRIBUTOR_STATE,
    mint: MINT,
    authority: wallet.publicKey,
    tokenAccount,
  }).rpc();

}

main();