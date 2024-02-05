// Migrations are an early feature. Currently, they're nothing more than this
// single deploy script that's invoked from the CLI, injecting a provider
// configured from the workspace's Anchor.toml.

import {Keypair, PublicKey} from "@solana/web3.js";
import {BN, Program} from "@coral-xyz/anchor";
import { Distributor } from "../target/types/distributor";
import * as fs from "fs";

const anchor = require("@coral-xyz/anchor");

module.exports = async function (provider) {
  // Configure client to use the provider.
  anchor.setProvider(provider);
  const program = anchor.workspace.Distributor as Program<Distributor>;

  const endpoint = provider.connection.rpcEndpoint;
  if (endpoint.includes("localhost") || endpoint.includes("devnet")) {
    await initDevnet(program, provider.wallet.publicKey);
  } else {
    await initMainnet(program, provider.wallet.publicKey);
  }
};

async function initDevnet(program: Program<Distributor>, payer: PublicKey) {
  const distributorAuthority = Keypair.fromSecretKey(new Uint8Array(JSON.parse(fs.readFileSync('keys/authority.json').toString()))).publicKey;
  const mint = Keypair.fromSecretKey(new Uint8Array(JSON.parse(fs.readFileSync('tests/keys/mint.json').toString()))).publicKey;
  const markerMint = Keypair.fromSecretKey(new Uint8Array(JSON.parse(fs.readFileSync('tests/keys/marker_mint.json').toString()))).publicKey;

  let shareSize = (new BN(331)).mul(new BN(1_000_000_000));
  let numberOfShares = new BN(10);

  let tx = await program.methods.initialize(shareSize, numberOfShares)
    .accounts({
      payer,
      mint,
      markerMint,
      distributorAuthority: distributorAuthority,
    })
    .rpc();
  console.log("Distributor initialized at ", tx);
}

async function initMainnet(program: Program<Distributor>, payer: PublicKey) {
    const distributorAuthority = Keypair.fromSecretKey(new Uint8Array(JSON.parse(fs.readFileSync('distributor-authority.json').toString()))).publicKey;
    const mint = new PublicKey("E2x5XH8eHkZGiaA8mFicU5CoGUJxSRKiXjEW5Nybf8Nn");
    const markerMint = new PublicKey("9gwTegFJJErDpWJKjPfLr2g2zrE3nL1v5zpwbtsk3c6P");

    let shareSize = (new BN(331)).mul(new BN(1_000_000_000));
    let numberOfShares = new BN(10);

    let tx = await program.methods.initialize(shareSize, numberOfShares)
        .accounts({
            payer,
            mint,
            markerMint,
            distributorAuthority: distributorAuthority,
        })
        .rpc();
    console.log("Distributor initialized at ", tx);
}