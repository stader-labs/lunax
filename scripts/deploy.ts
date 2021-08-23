import {
	LCDClient,
	getContractAddress,
	MsgExecuteContract,
	Coin, MsgStoreCode, MsgInstantiateContract, getCodeId, isTxError } from '@terra-money/terra.js';
import { client, wallet } from './clientAndWallet';
import { config } from './config';

const fs = require("fs");

/* this method uploads code of both stake-easy and stake-max
 */
async function uploadCode(): Promise<void> {
	const filepath = __dirname + "/../artifacts/stader_protocol_v0_cw20_test_token.wasm";
	// const filepath = __dirname + "/../artifacts/stader_protocol_v0.wasm";

	const storeCode = new MsgStoreCode(
		wallet.key.accAddress,
		fs.readFileSync(filepath).toString('base64')
	);
	console.log(wallet.key.accAddress);

	const storeCodeTx = await wallet.createAndSignTx({
		msgs: [storeCode],
	});
	const storeCodeTxResult = await client.tx.broadcast(storeCodeTx);

	console.log(storeCodeTxResult);

	if (isTxError(storeCodeTxResult)) {
		throw new Error(
			// "bar"
			// `store code failed. code: ${storeCodeTxResult.code}, codespace: ${storeCodeTxResult.codespace}, raw_log: ${storeCodeTxResult.raw_log}`
			`store code failed. code: ${storeCodeTxResult.code}, codespace: ${storeCodeTxResult.codespace}`
		);
	}

	const codeId = getCodeId(storeCodeTxResult);
	console.log(`code if is: ${codeId}`);

	// const codeId = 6237;
// 	const instantiate = new MsgInstantiateContract(
// 		wallet.key.accAddress,
// 		wallet.key.accAddress,
// 		+codeId, // code ID
// 		{
// 			min_deposit_per_user: "10000",
// 			max_deposit_per_user: "15000000",
// 			max_number_of_users: "10000",
// 			vault_denom: "uluna",
// 			// usable: "true",
// 			initial_validators: ["terravaloper1pfkp8qqha94vcahh5pll3hd8ujxu8j30xvlqmh", "terravaloper1peytphgvnmaz4fah8daww2yaugpw27cdkvcywa"],
// 		}, // InitMsg
// 		// { uluna: 10000000, ukrw: 1000000 } // init coins
// 		{ uluna: 10000000} // init coins
// 	);

//   const instantiate = new MsgInstantiateContract(
//     wallet.key.accAddress,
//     wallet.key.accAddress,
//     +codeId, // code ID
//     {
//       name: "stader-coin",
//       symbol: "sdc",
//       decimals: 2,
//       initial_balances: [],
//     }, // InitMsg
//     // { uluna: 10000000, ukrw: 1000000 } // init coins
//     { uluna: 10000000} // init coins
//   );
	// const codeId = 6356;
	const instantiate = new MsgInstantiateContract(
		wallet.key.accAddress,
		wallet.key.accAddress,
		+codeId, // code ID
		{
			min_deposit_per_user: "10000",
			max_deposit_per_user: "15000000",
			max_number_of_users: "10000",
			vault_denom: "uluna",
			// usable: "true",
			initial_validators: config.initialValidators,
		}, // InitMsg
		// { uluna: 10000000, ukrw: 1000000 } // init coins
		{ uluna: 10000000} // init coins
	);

	const instantiateTx = await wallet.createAndSignTx({
		msgs: [instantiate],
	});
	const instantiateTxResult = await client.tx.broadcast(instantiateTx);

	console.log(instantiateTxResult);

	if (isTxError(instantiateTxResult)) {
		throw new Error(
			// "foo"
			// `instantiate failed. code: ${instantiateTxResult.code}, codespace: ${instantiateTxResult.codespace}, raw_log: ${instantiateTxResult.raw_log}`
			`instantiate failed. code: ${instantiateTxResult.code}, codespace: ${instantiateTxResult.codespace}`
		);
	}

	const contractAddress = getContractAddress(instantiateTxResult);

	console.log(`Contract address is ${contractAddress}`)

	// const execute = new MsgExecuteContract(
	//   wallet.key.accAddress, // sender
	//   contractAddress, // contract address
	//   { increment: {} }, // handle msg
	//   { uluna: 100000 } // coins
	// );
	// const executeTx = await wallet.createAndSignTx({
	//   msgs: [execute],
	// });
	// const executeTxResult = await client.tx.broadcast(executeTx);
	// console.log(executeTxResult);
}

uploadCode().then(() => {
	console.log
},
	(err) => {
		// console.log(err.message)
		console.log(err);
	}
);