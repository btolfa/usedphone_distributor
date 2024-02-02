import * as anchor from "@coral-xyz/anchor";
import {Program, web3, BN, AnchorProvider} from "@coral-xyz/anchor";
import {splTokenProgram, SPL_TOKEN_PROGRAM_ID} from "@coral-xyz/spl-token";
import {PublicKey, Keypair, AccountMeta, ComputeBudgetProgram} from '@solana/web3.js';

import { Distributor } from "../target/types/distributor";

import {expect} from 'chai';
import * as chai from 'chai';
import chaiAsPromised from 'chai-as-promised';

import * as fs from "fs";
import {createMintIfRequired, createToken, getATA, mintTo} from "./utils";

chai.use(chaiAsPromised);

function deriveDistributorStateAddress(mint: PublicKey, markerMint: PublicKey, shareSize: BN, numberOfShares: BN, programId: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync([
    mint.toBuffer(), markerMint.toBuffer(), shareSize.toBuffer("le", 8), numberOfShares.toBuffer("le", 8)
  ], programId)[0];
}

function deriveVaultAddress(distributorState: PublicKey, programId: PublicKey): PublicKey {
  return PublicKey.findProgramAddressSync([distributorState.toBuffer()], programId)[0];
}

describe("distributor", () => {
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.Distributor as Program<Distributor>;
  const splProgram = splTokenProgram({
    provider,
    programId: SPL_TOKEN_PROGRAM_ID,
  });

  const distributorAuthority = Keypair.generate();

  const mint = Keypair.fromSecretKey(new Uint8Array(JSON.parse(fs.readFileSync('tests/keys/mint.json').toString())));
  const markerMint = Keypair.fromSecretKey(new Uint8Array(JSON.parse(fs.readFileSync('tests/keys/marker_mint.json').toString())));

  let shareSize = (new BN(331)).mul(new BN(1_000_000_000));
  let numberOfShares = new BN(10);

  const funderToken = Keypair.generate();

  before(async () => {
    await createMintIfRequired(splProgram, mint, provider.wallet.publicKey);
    await createMintIfRequired(splProgram, markerMint, provider.wallet.publicKey);

    await createToken(splProgram, funderToken, mint.publicKey, provider.wallet.publicKey);
    const supply = (new BN(1_000_000)).mul(new BN(1_000_000_000));
    await mintTo(splProgram, supply, mint.publicKey, funderToken.publicKey, provider.wallet.publicKey);
  });

  it("Should initialize", async () => {
    await program.methods.initialize(shareSize, numberOfShares)
      .accounts({
        payer: provider.wallet.publicKey,
        mint: mint.publicKey,
        markerMint: markerMint.publicKey,
        distributorAuthority: distributorAuthority.publicKey,
      })
      .rpc();
  });

  it("Should deposit by calling contract", async () => {
    const distributorState = deriveDistributorStateAddress(mint.publicKey, markerMint.publicKey, shareSize, numberOfShares, program.programId);
    const amount = shareSize.mul(numberOfShares.subn(1));
    await program.methods.deposit(amount).accounts({
      distributorState,
      mint: mint.publicKey,
      authority: provider.wallet.publicKey,
      tokenAccount: funderToken.publicKey,
    }).rpc();
  });

  it("Shouldn't distribute before threshold", async () => {
    const distributorState = deriveDistributorStateAddress(mint.publicKey, markerMint.publicKey, shareSize, numberOfShares, program.programId);

    const vaultAddress = deriveVaultAddress(distributorState, program.programId);
    const vaultAccount = await splProgram.account.account.fetch(vaultAddress);
    expect(vaultAccount.amount.toString()).to.equal((shareSize.mul(numberOfShares.subn(1))).toString());

    let remainingAccounts: AccountMeta[] = [];
    for (let i = 0; i < numberOfShares.toNumber() - 1; i++) {
      const receiver = Keypair.generate();
      const ata = getATA(receiver.publicKey, mint.publicKey);
      remainingAccounts.push({pubkey: receiver.publicKey, isWritable: false, isSigner: false});
      remainingAccounts.push({pubkey: ata, isWritable: true, isSigner: false});
    }

    await expect(program.methods.distribute()
      .accounts({
        payer: provider.wallet.publicKey,
        distributorAuthority: distributorAuthority.publicKey,
        distributorState,
        mint: mint.publicKey,
      }).remainingAccounts(remainingAccounts)
      .signers([distributorAuthority])
      .rpc()).to.be.rejected;
  });

  it("Should deposit by transferring tokens to vault", async () => {
    const distributorState = deriveDistributorStateAddress(mint.publicKey, markerMint.publicKey, shareSize, numberOfShares, program.programId);
    const vaultAddress = deriveVaultAddress(distributorState, program.programId);
    await splProgram.methods.transferChecked(shareSize, 9).accounts({
      source: funderToken.publicKey,
      mint: mint.publicKey,
      destination: vaultAddress,
      authority: provider.wallet.publicKey,
    }).rpc();
  });

  it("Should distribute if threshold is reached", async () => {
    const distributorState = deriveDistributorStateAddress(mint.publicKey, markerMint.publicKey, shareSize, numberOfShares, program.programId);

    const vaultAddress = deriveVaultAddress(distributorState, program.programId);
    const vaultAccount = await splProgram.account.account.fetch(vaultAddress);
    expect(vaultAccount.amount.toString()).to.equal((shareSize.mul(numberOfShares)).toString());

    let remainingAccounts: AccountMeta[] = [];
    for (let i = 0; i < numberOfShares.toNumber() - 1; i++) {
      const receiver = Keypair.generate();
      const ata = getATA(receiver.publicKey, mint.publicKey);
      remainingAccounts.push({pubkey: receiver.publicKey, isWritable: false, isSigner: false});
      remainingAccounts.push({pubkey: ata, isWritable: true, isSigner: false});
    }

    await program.methods.distribute()
      .accounts({
        payer: provider.wallet.publicKey,
        distributorAuthority: distributorAuthority.publicKey,
        distributorState,
        mint: mint.publicKey,
      }).remainingAccounts(remainingAccounts)
      .signers([distributorAuthority])
      .preInstructions([ComputeBudgetProgram.setComputeUnitLimit({units: 1_400_000})])
      .rpc({skipPreflight: true});
  });
});
