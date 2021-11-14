use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};

use pools::msg::{
    ExecuteMsg, GetAirdropMetaResponse, InstantiateMsg, QueryBatchUndelegationResponse,
    QueryConfigResponse, QueryMsg, QueryPoolResponse, QueryStateResponse,
};
use pools::state::Config;

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InstantiateMsg), &out_dir);
    export_schema(&schema_for!(ExecuteMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(Config), &out_dir);
    export_schema(&schema_for!(QueryConfigResponse), &out_dir);
    export_schema(&schema_for!(QueryStateResponse), &out_dir);
    export_schema(&schema_for!(QueryPoolResponse), &out_dir);
    export_schema(&schema_for!(QueryBatchUndelegationResponse), &out_dir);
    export_schema(&schema_for!(GetAirdropMetaResponse), &out_dir);
}
