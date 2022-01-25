import {
  isTxError,
  LCDClient,
  MnemonicKey,
  MsgExecuteContract,
} from "@terra-money/terra.js";

const REWARD_CONTRACT = "<Enter reward contract>";
const STAKING_CONTRACT = "<Enter staking contract>";
const CW20_TOKEN_CONTRACT = "<Enter cw20 token contract>"; // this is just a regular cw20 contract with staking_contract as minter
const MANAGER_MNEUMONIC_KEY = "<Add your mneumonic here!>";

const updateConfigs = async () => {
  const client = new LCDClient({
    URL: "https://bombay-fcd.terra.dev/",
    chainID: "bombay-12",
    gasPrices: { uusd: 0.15 },
  });
  const mk = new MnemonicKey({
    mnemonic: MANAGER_MNEUMONIC_KEY,
  });
  const wallet = client.wallet(mk);

  // update reward contract with staking contract
  const rewardExecuteTx = await wallet.createAndSignTx({
    msgs: [
      new MsgExecuteContract(wallet.key.accAddress, REWARD_CONTRACT, {
        update_config: {
          staking_contract: STAKING_CONTRACT,
        },
      }),
    ],
  });

  const rewardUpdateTxResult = await client.tx.broadcast(rewardExecuteTx);
  if (isTxError(rewardUpdateTxResult)) {
    throw new Error(`Failed to update reward contract`);
  }

  // update staking contract with cw20 mint
  const cw20UpdateExecuteTx = await wallet.createAndSignTx({
    msgs: [
      new MsgExecuteContract(wallet.key.accAddress, STAKING_CONTRACT, {
        update_config: {
          cw20_token_contract: CW20_TOKEN_CONTRACT,
        },
      }),
    ],
  });

  const cw20UpdateExecuteTxResult = await client.tx.broadcast(
    cw20UpdateExecuteTx
  );
  if (isTxError(cw20UpdateExecuteTxResult)) {
    throw new Error(`Failed to update reward contract`);
  }
};

updateConfigs().then(() => { })
