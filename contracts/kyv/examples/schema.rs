use std::env::current_dir;
use std::fs::create_dir_all;

use cosmwasm_schema::{export_schema, remove_schemas, schema_for};

use stader_terra_kyv::msg::{ExecuteMsg, InstantiateMsg, QueryMsg, ValidatorAprResponse};
use stader_terra_kyv::state::{Config, State, ValidatorMetrics};

fn main() {
    let mut out_dir = current_dir().unwrap();
    out_dir.push("schema");
    create_dir_all(&out_dir).unwrap();
    remove_schemas(&out_dir).unwrap();

    export_schema(&schema_for!(InstantiateMsg), &out_dir);
    export_schema(&schema_for!(ExecuteMsg), &out_dir);
    export_schema(&schema_for!(QueryMsg), &out_dir);
    export_schema(&schema_for!(State), &out_dir);
    export_schema(&schema_for!(Config), &out_dir);
    export_schema(&schema_for!(ValidatorMetrics), &out_dir);
    export_schema(&schema_for!(ValidatorAprResponse), &out_dir);
    // TODO: Make sure to add Schemas Here
}
