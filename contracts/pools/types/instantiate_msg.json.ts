{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "InstantiateMsg",
  "type": "object",
  "required": [
    "delegator_contract",
    "max_deposit",
    "min_deposit",
    "scc_contract",
    "vault_denom"
  ],
type "properties": = { 
properties": : { 
type "delegator_contract": = { 
delegator_contract": : { 
      "type": "string"
    
}};

type "max_deposit": = { 
max_deposit": : { 
      "$ref": "#/definitions/Uint128"
    
}};

type "min_deposit": = { 
min_deposit": : { 
      "$ref": "#/definitions/Uint128"
    
}};

type "scc_contract": = { 
scc_contract": : { 
      "type": "string"
    
}};

type "unbonding_buffer": = { 
unbonding_buffer": : { 
      "type": [
        "integer",
        "null"
      ],
      "format": "uint64",
      "minimum": 0.0
    
}};

type "unbonding_period": = { 
unbonding_period": : { 
      "type": [
        "integer",
        "null"
      ],
      "format": "uint64",
      "minimum": 0.0
    
}};

type "vault_denom": = { 
vault_denom": : { 
      "type": "string"
    }
  
}};

type "definitions": = { 
definitions": : { 
type "Uint128": = { 
_uint128": : { 
      "description": "A thin wrapper around u128 that is using strings for JSON encoding/decoding, such that the full u128 range can be used for clients that convert JSON numbers to floats, like JavaScript and jq.\n\n# Examples\n\nUse `from` to create instances of this and `u128` to get the value out:\n\n``` # use cosmwasm_std::Uint128; let a = Uint128::from(123u128); assert_eq!(a.u128(), 123);\n\nlet b = Uint128::from(42u64); assert_eq!(b.u128(), 42);\n\nlet c = Uint128::from(70u32); assert_eq!(c.u128(), 70); ```",
      "type": "string"
    }
  }
}

export type Instantiate_msg.json = "properties": | "delegator_contract": | "max_deposit": | "min_deposit": | "scc_contract": | "unbonding_buffer": | "unbonding_period": | "vault_denom": | "definitions": | "Uint128":;