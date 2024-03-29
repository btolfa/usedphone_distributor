# Distributor

### Prerequisites

- (Rust) [rustup](https://www.rust-lang.org/tools/install)
- (Solana) [solan-cli](https://docs.solana.com/cli/install-solana-cli-tools) 1.18.1
- (Anchor) [anchor](https://www.anchor-lang.com/docs/installation) 0.29.0
- (Node) [node](https://github.com/nvm-sh/nvm) 18.18.0

### Build and run tests

```bash
anchor build
anchor test --provider.cluster localnet
```

### Deploy to localnet

```bash
solana config set -u l

solana-test-validator -r --queit &

anchor build
cp keys/distributor-keypair.json target/deploy
anchor deploy --provider.cluster localnet
anchor idl init --filepath target/idl/distributor.json --provider.cluster localnet 5YP6jdWGTNDUhLYMCfocbyfT4RN58QbhVdtYmBdL6Af1

# Create mock tokens for tests
spl-token create-token --decimals 9 --mint-authority De49soBQoHpombVpexCsPEh7Fi5Pfh5fNbhKimhfG28i tests/keys/mint.json
spl-token create-token --decimals 9 --mint-authority De49soBQoHpombVpexCsPEh7Fi5Pfh5fNbhKimhfG28i tests/keys/marker_mint.json

# The follwoing command may fail
anchor migrate --provider.cluster localnet

# Run if anchor migrate failed
env ANCHOR_WALLET=~/.config/solana/id.json ./node_modules/.bin/ts-node .anchor/deploy.ts
```

### Deploy to devnet

```bash
solana config set -u d

anchor build
cp keys/distributor-keypair.json target/deploy
anchor deploy --provider.cluster devnet
anchor idl init --filepath target/idl/distributor.json --provider.cluster devnet 5YP6jdWGTNDUhLYMCfocbyfT4RN58QbhVdtYmBdL6Af1

spl-token create-token --decimals 9 --mint-authority De49soBQoHpombVpexCsPEh7Fi5Pfh5fNbhKimhfG28i tests/keys/mint.json
spl-token create-token --decimals 9 --mint-authority De49soBQoHpombVpexCsPEh7Fi5Pfh5fNbhKimhfG28i tests/keys/marker_mint.json

# The follwoing command may fail
anchor migrate --provider.cluster devnet

# Run if anchor migrate failed
env ANCHOR_WALLET=~/.config/solana/id.json ./node_modules/.bin/ts-node .anchor/deploy.ts
```

#### localnet/devnet settings 

programId [5YP6jdWGTNDUhLYMCfocbyfT4RN58QbhVdtYmBdL6Af1](https://explorer.solana.com/address/5YP6jdWGTNDUhLYMCfocbyfT4RN58QbhVdtYmBdL6Af1?cluster=devnet)

```
mint 6VzNqK5a68KTqfkC2xRzF3fH5kKKAA8D2xdPCe6FDxS7
marker mint 9Dysc3vtrrYr7eaFX3gosGfyJgKxzQmZs47pmhWMG1P
mint-authority De49soBQoHpombVpexCsPEh7Fi5Pfh5fNbhKimhfG28i (keys/authority.json)
```

DistributorState [EBHnjoKTCn4S27pYsfYesRbnVr3JmAHg6E5JEnrgAqCR](https://explorer.solana.com/address/EBHnjoKTCn4S27pYsfYesRbnVr3JmAHg6E5JEnrgAqCR?cluster=devnet)

Vault [4wZ2E3St33iB5xu9R2Kf6NbMa5pkoeqVNe1SkcFVvoX5](https://explorer.solana.com/address/4wZ2E3St33iB5xu9R2Kf6NbMa5pkoeqVNe1SkcFVvoX5?cluster=devnet)

#### How to mint and fund pools

```bash
spl-token create-account 6VzNqK5a68KTqfkC2xRzF3fH5kKKAA8D2xdPCe6FDxS7
spl-token mint 6VzNqK5a68KTqfkC2xRzF3fH5kKKAA8D2xdPCe6FDxS7 10000000000 --mint-authority keys/authority.json <TOKEN-ACCOUNT>
spl-token transfer 6VzNqK5a68KTqfkC2xRzF3fH5kKKAA8D2xdPCe6FDxS7 1000000000 <TOKEN-ACCOUNT>
```

### Deploy to mainnet

```bash
solana config set --url mainnet-beta

anchor build
cp keys/distributor-keypair.json target/deploy
solana --config <config> program deploy target/deploy/staking.so --max-len <size>
solana --config <config> program set-upgrade-authority <old-upgrade-authority> --new-upgrade-authority <new-upgrade-authority>

anchor migrate --provider.cluster mainnet
env ANCHOR_WALLET=~/crypton/csm_deploy.json ./node_modules/.bin/ts-node .anchor/deploy.ts
```

#### mainnet settings

programId [5YP6jdWGTNDUhLYMCfocbyfT4RN58QbhVdtYmBdL6Af1](https://explorer.solana.com/address/5YP6jdWGTNDUhLYMCfocbyfT4RN58QbhVdtYmBdL6Af1)

```
mint E2x5XH8eHkZGiaA8mFicU5CoGUJxSRKiXjEW5Nybf8Nn
marker mint 9gwTegFJJErDpWJKjPfLr2g2zrE3nL1v5zpwbtsk3c6P
```

DistributorState [9eiohYGDXQA2KJW1VdSEm2fZt2PVNA9ujQAMaeANxQPo](https://explorer.solana.com/address/9eiohYGDXQA2KJW1VdSEm2fZt2PVNA9ujQAMaeANxQPo)
Vault [AFvsmxNq9maU5daAmxDuvPeJtEpfnMXnSpwrEgcVyen8](https://explorer.solana.com/address/AFvsmxNq9maU5daAmxDuvPeJtEpfnMXnSpwrEgcVyen8)
DistributorAuthority [6Ub4CxXEgBvx7ni9m88s9Uws29zgErVF8dMrPz2EH45e](https://explorer.solana.com/address/6Ub4CxXEgBvx7ni9m88s9Uws29zgErVF8dMrPz2EH45e)