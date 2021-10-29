{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "QueryPoolResponse",
  "type": "object",
type "properties": = { 
properties": : { 
type "pool": = { 
pool": : { 
      "anyOf": [
type { = { 
 : { 
          "$ref": "#/definitions/PoolRegistryInfo"
        
}};

type { = { 
 : { 
          "type": "null"
        }
      ]
    }
  
}};

type "definitions": = { 
definitions": : { 
type "Addr": = { 
_addr": : { 
      "description": "A human readable address.\n\nIn Cosmos, this is typically bech32 encoded. But for multi-chain smart contracts no assumptions should be made other than being UTF-8 encoded and of reasonable length.\n\nThis type represents a validated address. It can be created in the following ways 1. Use `Addr::unchecked(input)` 2. Use `let checked: Addr = deps.api.addr_validate(input)?` 3. Use `let checked: Addr = deps.api.addr_humanize(canonical_addr)?` 4. Deserialize from JSON. This must only be done from JSON that was validated before such as a contract's state. `Addr` must not be used in messages sent by the user because this would result in unvalidated instances.\n\nThis type is immutable. If you really need to mutate it (Really? Are you sure?), create a mutable copy using `let mut mutable = Addr::to_string()` and operate on that `string` instance.",
      "type": "string"
    
}};

type "DecCoin": = { 
_dec_coin": : { 
      "type": "object",
      "required": [
        "amount",
        "denom"
      ],
type "properties": = { 
properties": : { 
type "amount": = { 
amount": : { 
          "$ref": "#/definitions/Decimal"
        
}};

type "denom": = { 
denom": : { 
          "type": "string"
        }
      }
    
}};

type "Decimal": = { 
_decimal": : { 
      "description": "A fixed-point decimal value with 18 fractional digits, i.e. Decimal(1_000_000_000_000_000_000) == 1.0\n\nThe greatest possible value that can be represented is 340282366920938463463.374607431768211455 (which is (2^128 - 1) / 10^18)",
      "type": "string"
    
}};

type "PoolRegistryInfo": = { 
_pool_registry_info": : { 
      "type": "object",
      "required": [
        "active",
        "airdrops_pointer",
        "current_undelegation_batch_id",
        "last_reconciled_batch_id",
        "name",
        "protocol_fee_contract",
        "protocol_fee_percent",
        "reward_contract",
        "rewards_pointer",
        "slashing_pointer",
        "staked",
        "validator_contract",
        "validators"
      ],
type "properties": = { 
properties": : { 
type "active": = { 
active": : { 
          "type": "boolean"
        
}};

type "airdrops_pointer": = { 
airdrops_pointer": : { 
          "type": "array",
type "items": = { 
items": : { 
            "$ref": "#/definitions/DecCoin"
          }
        
}};

type "current_undelegation_batch_id": = { 
current_undelegation_batch_id": : { 
          "type": "integer",
          "format": "uint64",
          "minimum": 0.0
        
}};

type "last_reconciled_batch_id": = { 
last_reconciled_batch_id": : { 
          "type": "integer",
          "format": "uint64",
          "minimum": 0.0
        
}};

type "name": = { 
name": : { 
          "type": "string"
        
}};

type "protocol_fee_contract": = { 
protocol_fee_contract": : { 
          "$ref": "#/definitions/Addr"
        
}};

type "protocol_fee_percent": = { 
protocol_fee_percent": : { 
          "$ref": "#/definitions/Decimal"
        
}};

type "reward_contract": = { 
reward_contract": : { 
          "$ref": "#/definitions/Addr"
        
}};

type "rewards_pointer": = { 
rewards_pointer": : { 
          "$ref": "#/definitions/Decimal"
        
}};

type "slashing_pointer": = { 
slashing_pointer": : { 
          "$ref": "#/definitions/Decimal"
        
}};

type "staked": = { 
staked": : { 
          "$ref": "#/definitions/Uint128"
        
}};

type "validator_contract": = { 
validator_contract": : { 
          "$ref": "#/definitions/Addr"
        
}};

type "validators": = { 
validators": : { 
          "type": "array",
type "items": = { 
items": : { 
            "$ref": "#/definitions/Addr"
          }
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

export type Query_pool_response.json = "properties": | "pool": | { | { | "definitions": | "Addr": | "DecCoin": | "properties": | "amount": | "denom": | "Decimal": | "PoolRegistryInfo": | "properties": | "active": | "airdrops_pointer": | "items": | "current_undelegation_batch_id": | "last_reconciled_batch_id": | "name": | "protocol_fee_contract": | "protocol_fee_percent": | "reward_contract": | "rewards_pointer": | "slashing_pointer": | "staked": | "validator_contract": | "validators": | "items": | "Uint128":;