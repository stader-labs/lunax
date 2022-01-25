Deployment scripts info

To deploy lunax, please execute the deploy.ts and then updateConfigs.ts in order. 

There are some details required which have been marked in "<>" in the scripts. These are basic details like manager key mneumonic and stuff.

The cw20 token contract to deploy is a simple cw20 token contract from cw-plus repo https://github.com/CosmWasm/cw-plus. The minter should be set to the staking contract.