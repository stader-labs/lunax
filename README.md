# LunaX - Liquid staking with crazy yields on Terra 2.0

LunaX is a liquid staking solution offered by [Staderlabs](https://staderlabs.com "Staderlabs") to the Terra 2.0 ecosystem. LunaX unlocks your staked Luna and enables you to participate in various DeFi protocols like Terraswap, Astroport etc to get yields on top of your staking rewards! Plus the Luna staked is distributed equally across our validator pool, so you are also contributing towards decentralization of the Terra 2.0 ecosystem! Double win!

### Contracts in the repo

The LunaX contracts are built with [cosmwasm 1.0.0](https://github.com/CosmWasm "cosmwasm 1.0.0") . The following are the contracts in the repo

1. Airdrops registry: This contract is a registry contract of various airdrop contracts. This is a central store for Stader to query the cw20 contracts of various airdrop tokens. 

2. Reward: This contract collects staking rewards from the staking pool. All the staking rewards collected are sent to this contract.

3. Staking: This is the main staking contract which the user interacts with. users interact with this contract to deposit Luna and mint LunaX. Similarly, users interact with this contract to burn LunaX and unstake their stake which takes 21 days to release. Post the 21 day unstaking period users, users interact with this contract to withdraw their stake. This contract also contains messages required for validator stake pool operations. 

### Building the project

To build the project for a production release, run the following command in the root directory of the project:

```bash
docker run --rm -v "$(pwd)":/code \
  -v /run/host-services/ssh-auth.sock:/run/host-services/ssh-auth.sock \
  -e SSH_AUTH_SOCK="/run/host-services/ssh-auth.sock" \
  --mount type=volume,source="$(basename "$(pwd)")_cache",target=/code/target \
  --mount type=volume,source=registry_cache,target=/usr/local/cargo/registry \
  cosmwasm/workspace-optimizer:0.12.10
```

The optimized wasms get saved in the artifacts directory. 

You can also go into each directory and run

```bash
cargo build

```
To run tests, you can go into each contract directory and run

```bash
cargo test
```

### Live contracts

The following are the contracts on mainnet:

1. Staking contract: terra179e90rqspswfzmhdl25tg22he0fcefwndgzc957ncx9dleduu7ms3evpuk

2. Reward contract: terra1sstqldl7tyvvdseppsa022acrxp7cuuplkc7639w7x7cmm4hjvvqjpwh0x

3. Airdrop registry: terra1fvw0rt94gl5eyeq36qdhj5x7lunv3xpuqcjxa0llhdssvqtcmrnqlzxdyr

4. LunaX Cw20 Token: terra14xsm2wzvu7xaf567r693vgfkhmvfs08l68h4tjj5wjgyn5ky8e2qvzyanh

### Dapp link

https://terra.staderlabs.com/liquid-staking

