use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};
use staking::msg::*;
use staking::state::*;

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InstantiateMsg), &out_dir);
    export_schema(&schema_for!(ExecuteMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(Cw20HookMsg), &out_dir);
    export_schema(&schema_for!(MerkleAirdropMsg), &out_dir);
    export_schema(&schema_for!(Config), &out_dir);
    export_schema(&schema_for!(QueryConfigResponse), &out_dir);
    export_schema(&schema_for!(QueryStateResponse), &out_dir);
    export_schema(&schema_for!(QueryBatchUndelegationResponse), &out_dir);
    export_schema(&schema_for!(GetValMetaResponse), &out_dir);
    export_schema(&schema_for!(GetFundsClaimRecord), &out_dir);

}
