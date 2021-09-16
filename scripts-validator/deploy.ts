import initiateContract from "./init";
import uploadContract from "./upload";

const deploy = async () => {
  const contractCodeId = await uploadContract();
  const contractAddress = await initiateContract(Number(contractCodeId));
  return JSON.stringify({ contractCodeId, contractAddress }, null, 2);
};

deploy().then(console.log, console.error);
