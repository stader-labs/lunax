{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "QueryConfigResponse",
  "type": "object",
  "required": [
    "config"
  ],
type "properties": = { 
properties": : { 
type "config": = { 
config": : { 
      "$ref": "#/definitions/Config"
    }
  
}};

type "definitions": = { 
definitions": : { 
type "Addr": = { 
_addr": : { 
      "description": "A human readable address.\n\nIn Cosmos, this is typically bech32 encoded. But for multi-chain smart contracts no assumptions should be made other than being UTF-8 encoded and of reasonable length.\n\nThis type represents a validated address. It can be created in the following ways 1. Use `Addr::unchecked(input)` 2. Use `let checked: Addr = deps.api.addr_validate(input)?` 3. Use `let checked: Addr = deps.api.addr_humanize(canonical_addr)?` 4. Deserialize from JSON. This must only be done from JSON that was validated before such as a contract's state. `Addr` must not be used in messages sent by the user because this would result in unvalidated instances.\n\nThis type is immutable. If you really need to mutate it (Really? Are you sure?), create a mutable copy using `let mut mutable = Addr::to_string()` and operate on that `string` instance.",
      "type": "string"
    
}};

type "Config": = { 
_config": : { 
      "type": "object",
      "required": [
        "delegator_contract",
        "manager",
        "max_deposit",
        "min_deposit",
        "scc_contract",
        "unbonding_buffer",
        "unbonding_period",
        "vault_denom"
      ],
type "properties": = { 
properties": : { 
type "delegator_contract": = { 
delegator_contract": : { 
          "$ref": "#/definitions/Addr"
        
}};

type "manager": = { 
manager": : { 
          "$ref": "#/definitions/Addr"
        
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
          "$ref": "#/definitions/Addr"
        
}};

type "unbonding_buffer": = { 
unbonding_buffer": : { 
          "type": "integer",
          "format": "uint64",
          "minimum": 0.0
        
}};

type "unbonding_period": = { 
unbonding_period": : { 
          "type": "integer",
          "format": "uint64",
          "minimum": 0.0
        
}};

type "vault_denom": = { 
vault_denom": : { 
          "type": "string"
        }
      }
    
}};

type "Uint128": = { 
_uint128": : { 
      "description": "A thin wrapper around u128 that is using strings for JSON encoding/decoding, such that the full u128 range can be used for clients that convert JSON numbers to floats, like JavaScript and jq.\n\n# Examples\n\nUse `from` to create instances of this and `u128` to get the value out:\n\n``` # use cosmwasm_std::Uint128; let a = Uint128::from(123u128); assert_eq!(a.u128(), 123);\n\nlet b = Uint128::from(42u64); assert_eq!(b.u128(), 42);\n\nlet c = Uint128::from(70u32); assert_eq!(c.u128(), 70); ```",
      "type": "string"
    }
  }
}

export type Query_config_response.json = "properties": | "config": | "definitions": | "Addr": | "Config": | "properties": | "delegator_contract": | "manager": | "max_deposit": | "min_deposit": | "scc_contract": | "unbonding_buffer": | "unbonding_period": | "vault_denom": | "Uint128":;