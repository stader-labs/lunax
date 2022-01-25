import {
  getCodeId,
  getContractAddress,
  isTxError,
  LCDClient,
  MnemonicKey,
  MsgInstantiateContract,
  MsgStoreCode,
} from "@terra-money/terra.js";

const fs = require("fs");

const MANAGER_MNEUMONIC_KEY = "<Add your mneumonic here!>";
const PROTOCOL_FEE_CONTRACT =
  "<Add any terra wallet address or contract address>";
const AIRDROP_WITHDRAWAL_CONTRACT =
  "<Add any terra wallet address or contract address>";
// this is because of cyclic deps in reward and airdrops sink contract. Ideally we should have marked
// staking_contarct in these 2 contracts with an Addr::unchecked("0")
const TEMP_ADDRESS = "<Enter a temp terra address>";

let contractInfoMap: Record<
  string,
  {
    address: string;
    codeId: string;
  }
>;

const deployContract = async (deploymentInfo: {
  contractName: string;
  filePath: string;
  initMsg: object;
}) => {
  const client = new LCDClient({
    URL: "https://bombay-fcd.terra.dev/",
    chainID: "bombay-12",
    gasPrices: { uusd: 0.15 },
  });
  const mk = new MnemonicKey({
    mnemonic: MANAGER_MNEUMONIC_KEY,
  });
  const wallet = client.wallet(mk);

  const storeCode = new MsgStoreCode(
    wallet.key.accAddress,
    fs.readFileSync(deploymentInfo.filePath).toString("base64")
  );
  const storeCodeTx = await wallet.createAndSignTx({
    msgs: [storeCode],
  });
  const storeCodeTxResult = await client.tx.broadcast(storeCodeTx);

  if (isTxError(storeCodeTxResult)) {
    throw new Error(
      `store code failed. code: ${storeCodeTxResult.code}, codespace: ${storeCodeTxResult.codespace}`
    );
  }

  const codeId = getCodeId(storeCodeTxResult);

  const instantiate = new MsgInstantiateContract(
    wallet.key.accAddress,
    wallet.key.accAddress,
    +codeId,
    deploymentInfo.initMsg
  );

  const instantiateTx = await wallet.createAndSignTx({
    msgs: [instantiate],
  });
  const instantiateTxResult = await client.tx.broadcast(instantiateTx);

  if (isTxError(instantiateTxResult)) {
    throw new Error(
      `instantiate failed. code: ${instantiateTxResult.code}, codespace: ${instantiateTxResult.codespace}`
    );
  }
  const address = getContractAddress(instantiateTxResult);

  contractInfoMap[deploymentInfo.contractName] = {
    codeId,
    address,
  };

  console.log(
    `Deployed ${deploymentInfo.contractName} with codeId: ${codeId} and address: ${address}`
  );
};

const deploy = async () => {
  const reward_contract_wasm = `${__dirname}/../artifacts/reward.wasm`;
  const airdrops_registry_wasm = `${__dirname}/../artifacts/airdrops_registry.wasm`;
  const staking_wasm = `${__dirname}/../artifacts/staking.wasm`;

  await deployContract({
    contractName: "Reward",
    filePath: reward_contract_wasm,
    initMsg: {
      staking_contract: TEMP_ADDRESS,
    },
  });
  await deployContract({
    contractName: "Airdrops Registry",
    filePath: airdrops_registry_wasm,
    initMsg: {},
  });
  await deployContract({
    contractName: "Staking",
    filePath: staking_wasm,
    initMsg: {
      min_deposit: "10",
      max_deposit: "100000000",

      reward_contract: contractInfoMap["Reward"],
      airdrops_registry_contract: contractInfoMap["Airdrops Registry"],
      airdrop_withdrawal_contract: AIRDROP_WITHDRAWAL_CONTRACT,

      protocol_fee_contract: PROTOCOL_FEE_CONTRACT,
      protocol_reward_fee: "0.01", // "1 is 100%, 0.02 is 2%"
      protocol_deposit_fee: "0.01",
      protocol_withdraw_fee: "0.01", // "1 is 100%, 0.02 is 2%"

      unbonding_period: 86400,
      undelegation_cooldown: 100,
      swap_cooldown: 100,
      reinvest_cooldown: 100,
    },
  });
};

deploy().then(() => {});
