{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "ExecuteMsg",
  "oneOf": [
type { = { 
 : { 
      "type": "object",
      "required": [
        "add_pool"
      ],
type "properties": = { 
properties": : { 
type "add_pool": = { 
add_pool": : { 
          "type": "object",
          "required": [
            "name",
            "protocol_fee_contract",
            "protocol_fee_percent",
            "reward_contract",
            "validator_contract"
          ],
type "properties": = { 
properties": : { 
type "name": = { 
name": : { 
              "type": "string"
            
}};

type "protocol_fee_contract": = { 
protocol_fee_contract": : { 
              "type": "string"
            
}};

type "protocol_fee_percent": = { 
protocol_fee_percent": : { 
              "$ref": "#/definitions/Decimal"
            
}};

type "reward_contract": = { 
reward_contract": : { 
              "type": "string"
            
}};

type "validator_contract": = { 
validator_contract": : { 
              "type": "string"
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "add_validator"
      ],
type "properties": = { 
properties": : { 
type "add_validator": = { 
add_validator": : { 
          "type": "object",
          "required": [
            "pool_id",
            "val_addr"
          ],
type "properties": = { 
properties": : { 
type "pool_id": = { 
pool_id": : { 
              "type": "integer",
              "format": "uint64",
              "minimum": 0.0
            
}};

type "val_addr": = { 
val_addr": : { 
              "$ref": "#/definitions/Addr"
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "remove_validator"
      ],
type "properties": = { 
properties": : { 
type "remove_validator": = { 
remove_validator": : { 
          "type": "object",
          "required": [
            "pool_id",
            "redel_addr",
            "val_addr"
          ],
type "properties": = { 
properties": : { 
type "pool_id": = { 
pool_id": : { 
              "type": "integer",
              "format": "uint64",
              "minimum": 0.0
            
}};

type "redel_addr": = { 
redel_addr": : { 
              "$ref": "#/definitions/Addr"
            
}};

type "val_addr": = { 
val_addr": : { 
              "$ref": "#/definitions/Addr"
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "rebalance_pool"
      ],
type "properties": = { 
properties": : { 
type "rebalance_pool": = { 
rebalance_pool": : { 
          "type": "object",
          "required": [
            "amount",
            "pool_id",
            "redel_addr",
            "val_addr"
          ],
type "properties": = { 
properties": : { 
type "amount": = { 
amount": : { 
              "$ref": "#/definitions/Uint128"
            
}};

type "pool_id": = { 
pool_id": : { 
              "type": "integer",
              "format": "uint64",
              "minimum": 0.0
            
}};

type "redel_addr": = { 
redel_addr": : { 
              "$ref": "#/definitions/Addr"
            
}};

type "val_addr": = { 
val_addr": : { 
              "$ref": "#/definitions/Addr"
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "deposit"
      ],
type "properties": = { 
properties": : { 
type "deposit": = { 
deposit": : { 
          "type": "object",
          "required": [
            "pool_id"
          ],
type "properties": = { 
properties": : { 
type "pool_id": = { 
pool_id": : { 
              "type": "integer",
              "format": "uint64",
              "minimum": 0.0
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "redeem_rewards"
      ],
type "properties": = { 
properties": : { 
type "redeem_rewards": = { 
redeem_rewards": : { 
          "type": "object",
          "required": [
            "pool_id"
          ],
type "properties": = { 
properties": : { 
type "pool_id": = { 
pool_id": : { 
              "type": "integer",
              "format": "uint64",
              "minimum": 0.0
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "swap"
      ],
type "properties": = { 
properties": : { 
type "swap": = { 
swap": : { 
          "type": "object",
          "required": [
            "pool_id"
          ],
type "properties": = { 
properties": : { 
type "pool_id": = { 
pool_id": : { 
              "type": "integer",
              "format": "uint64",
              "minimum": 0.0
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "send_rewards_to_scc"
      ],
type "properties": = { 
properties": : { 
type "send_rewards_to_scc": = { 
send_rewards_to_scc": : { 
          "type": "object",
          "required": [
            "pool_id"
          ],
type "properties": = { 
properties": : { 
type "pool_id": = { 
pool_id": : { 
              "type": "integer",
              "format": "uint64",
              "minimum": 0.0
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "queue_undelegate"
      ],
type "properties": = { 
properties": : { 
type "queue_undelegate": = { 
queue_undelegate": : { 
          "type": "object",
          "required": [
            "amount",
            "pool_id"
          ],
type "properties": = { 
properties": : { 
type "amount": = { 
amount": : { 
              "$ref": "#/definitions/Uint128"
            
}};

type "pool_id": = { 
pool_id": : { 
              "type": "integer",
              "format": "uint64",
              "minimum": 0.0
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "undelegate"
      ],
type "properties": = { 
properties": : { 
type "undelegate": = { 
undelegate": : { 
          "type": "object",
          "required": [
            "pool_id"
          ],
type "properties": = { 
properties": : { 
type "pool_id": = { 
pool_id": : { 
              "type": "integer",
              "format": "uint64",
              "minimum": 0.0
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "reconcile_funds"
      ],
type "properties": = { 
properties": : { 
type "reconcile_funds": = { 
reconcile_funds": : { 
          "type": "object",
          "required": [
            "pool_id"
          ],
type "properties": = { 
properties": : { 
type "pool_id": = { 
pool_id": : { 
              "type": "integer",
              "format": "uint64",
              "minimum": 0.0
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "withdraw_funds_to_wallet"
      ],
type "properties": = { 
properties": : { 
type "withdraw_funds_to_wallet": = { 
withdraw_funds_to_wallet": : { 
          "type": "object",
          "required": [
            "batch_id",
            "pool_id",
            "undelegate_id"
          ],
type "properties": = { 
properties": : { 
type "batch_id": = { 
batch_id": : { 
              "type": "integer",
              "format": "uint64",
              "minimum": 0.0
            
}};

type "pool_id": = { 
pool_id": : { 
              "type": "integer",
              "format": "uint64",
              "minimum": 0.0
            
}};

type "undelegate_id": = { 
undelegate_id": : { 
              "type": "integer",
              "format": "uint64",
              "minimum": 0.0
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "update_airdrop_registry"
      ],
type "properties": = { 
properties": : { 
type "update_airdrop_registry": = { 
update_airdrop_registry": : { 
          "type": "object",
          "required": [
            "airdrop_contract",
            "airdrop_token",
            "cw20_contract"
          ],
type "properties": = { 
properties": : { 
type "airdrop_contract": = { 
airdrop_contract": : { 
              "type": "string"
            
}};

type "airdrop_token": = { 
airdrop_token": : { 
              "type": "string"
            
}};

type "cw20_contract": = { 
cw20_contract": : { 
              "type": "string"
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "claim_airdrops"
      ],
type "properties": = { 
properties": : { 
type "claim_airdrops": = { 
claim_airdrops": : { 
          "type": "object",
          "required": [
            "rates"
          ],
type "properties": = { 
properties": : { 
type "rates": = { 
rates": : { 
              "type": "array",
type "items": = { 
items": : { 
                "$ref": "#/definitions/AirdropRate"
              }
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "update_config"
      ],
type "properties": = { 
properties": : { 
type "update_config": = { 
update_config": : { 
          "type": "object",
          "required": [
            "config_request"
          ],
type "properties": = { 
properties": : { 
type "config_request": = { 
config_request": : { 
              "$ref": "#/definitions/ConfigUpdateRequest"
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "update_pool_metadata"
      ],
type "properties": = { 
properties": : { 
type "update_pool_metadata": = { 
update_pool_metadata": : { 
          "type": "object",
          "required": [
            "pool_config_update_request",
            "pool_id"
          ],
type "properties": = { 
properties": : { 
type "pool_config_update_request": = { 
pool_config_update_request": : { 
              "$ref": "#/definitions/PoolConfigUpdateRequest"
            
}};

type "pool_id": = { 
pool_id": : { 
              "type": "integer",
              "format": "uint64",
              "minimum": 0.0
            }
          }
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "simulate_slashing"
      ],
type "properties": = { 
properties": : { 
type "simulate_slashing": = { 
simulate_slashing": : { 
          "type": "object",
          "required": [
            "amount",
            "pool_id",
            "val_addr"
          ],
type "properties": = { 
properties": : { 
type "amount": = { 
amount": : { 
              "$ref": "#/definitions/Uint128"
            
}};

type "pool_id": = { 
pool_id": : { 
              "type": "integer",
              "format": "uint64",
              "minimum": 0.0
            
}};

type "val_addr": = { 
val_addr": : { 
              "$ref": "#/definitions/Addr"
            }
          }
        }
      
}};

      "additionalProperties": false
    }
  ],
type "definitions": = { 
definitions": : { 
type "Addr": = { 
_addr": : { 
      "description": "A human readable address.\n\nIn Cosmos, this is typically bech32 encoded. But for multi-chain smart contracts no assumptions should be made other than being UTF-8 encoded and of reasonable length.\n\nThis type represents a validated address. It can be created in the following ways 1. Use `Addr::unchecked(input)` 2. Use `let checked: Addr = deps.api.addr_validate(input)?` 3. Use `let checked: Addr = deps.api.addr_humanize(canonical_addr)?` 4. Deserialize from JSON. This must only be done from JSON that was validated before such as a contract's state. `Addr` must not be used in messages sent by the user because this would result in unvalidated instances.\n\nThis type is immutable. If you really need to mutate it (Really? Are you sure?), create a mutable copy using `let mut mutable = Addr::to_string()` and operate on that `string` instance.",
      "type": "string"
    
}};

type "AirdropRate": = { 
_airdrop_rate": : { 
      "type": "object",
      "required": [
        "amount",
        "claim_msg",
        "denom",
        "pool_id"
      ],
type "properties": = { 
properties": : { 
type "amount": = { 
amount": : { 
          "$ref": "#/definitions/Uint128"
        
}};

type "claim_msg": = { 
claim_msg": : { 
          "$ref": "#/definitions/Binary"
        
}};

type "denom": = { 
denom": : { 
          "type": "string"
        
}};

type "pool_id": = { 
pool_id": : { 
          "type": "integer",
          "format": "uint64",
          "minimum": 0.0
        }
      }
    
}};

type "Binary": = { 
_binary": : { 
      "description": "Binary is a wrapper around Vec<u8> to add base64 de/serialization with serde. It also adds some helper methods to help encode inline.\n\nThis is only needed as serde-json-{core,wasm} has a horrible encoding for Vec<u8>",
      "type": "string"
    
}};

type "ConfigUpdateRequest": = { 
_config_update_request": : { 
      "type": "object",
type "properties": = { 
properties": : { 
type "delegator_contract": = { 
delegator_contract": : { 
          "anyOf": [
type { = { 
 : { 
              "$ref": "#/definitions/Addr"
            
}};

type { = { 
 : { 
              "type": "null"
            }
          ]
        
}};

type "max_deposit": = { 
max_deposit": : { 
          "anyOf": [
type { = { 
 : { 
              "$ref": "#/definitions/Uint128"
            
}};

type { = { 
 : { 
              "type": "null"
            }
          ]
        
}};

type "min_deposit": = { 
min_deposit": : { 
          "anyOf": [
type { = { 
 : { 
              "$ref": "#/definitions/Uint128"
            
}};

type { = { 
 : { 
              "type": "null"
            }
          ]
        
}};

type "scc_contract": = { 
scc_contract": : { 
          "anyOf": [
type { = { 
 : { 
              "$ref": "#/definitions/Addr"
            
}};

type { = { 
 : { 
              "type": "null"
            }
          ]
        
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
        }
      }
    
}};

type "Decimal": = { 
_decimal": : { 
      "description": "A fixed-point decimal value with 18 fractional digits, i.e. Decimal(1_000_000_000_000_000_000) == 1.0\n\nThe greatest possible value that can be represented is 340282366920938463463.374607431768211455 (which is (2^128 - 1) / 10^18)",
      "type": "string"
    
}};

type "PoolConfigUpdateRequest": = { 
_pool_config_update_request": : { 
      "type": "object",
type "properties": = { 
properties": : { 
type "active": = { 
active": : { 
          "type": [
            "boolean",
            "null"
          ]
        
}};

type "protocol_fee_contract": = { 
protocol_fee_contract": : { 
          "type": [
            "string",
            "null"
          ]
        
}};

type "protocol_fee_percent": = { 
protocol_fee_percent": : { 
          "anyOf": [
type { = { 
 : { 
              "$ref": "#/definitions/Decimal"
            
}};

type { = { 
 : { 
              "type": "null"
            }
          ]
        
}};

type "reward_contract": = { 
reward_contract": : { 
          "type": [
            "string",
            "null"
          ]
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

export type Execute_msg.json = { | "properties": | "add_pool": | "properties": | "name": | "protocol_fee_contract": | "protocol_fee_percent": | "reward_contract": | "validator_contract": | { | "properties": | "add_validator": | "properties": | "pool_id": | "val_addr": | { | "properties": | "remove_validator": | "properties": | "pool_id": | "redel_addr": | "val_addr": | { | "properties": | "rebalance_pool": | "properties": | "amount": | "pool_id": | "redel_addr": | "val_addr": | { | "properties": | "deposit": | "properties": | "pool_id": | { | "properties": | "redeem_rewards": | "properties": | "pool_id": | { | "properties": | "swap": | "properties": | "pool_id": | { | "properties": | "send_rewards_to_scc": | "properties": | "pool_id": | { | "properties": | "queue_undelegate": | "properties": | "amount": | "pool_id": | { | "properties": | "undelegate": | "properties": | "pool_id": | { | "properties": | "reconcile_funds": | "properties": | "pool_id": | { | "properties": | "withdraw_funds_to_wallet": | "properties": | "batch_id": | "pool_id": | "undelegate_id": | { | "properties": | "update_airdrop_registry": | "properties": | "airdrop_contract": | "airdrop_token": | "cw20_contract": | { | "properties": | "claim_airdrops": | "properties": | "rates": | "items": | { | "properties": | "update_config": | "properties": | "config_request": | { | "properties": | "update_pool_metadata": | "properties": | "pool_config_update_request": | "pool_id": | { | "properties": | "simulate_slashing": | "properties": | "amount": | "pool_id": | "val_addr": | "definitions": | "Addr": | "AirdropRate": | "properties": | "amount": | "claim_msg": | "denom": | "pool_id": | "Binary": | "ConfigUpdateRequest": | "properties": | "delegator_contract": | { | { | "max_deposit": | { | { | "min_deposit": | { | { | "scc_contract": | { | { | "unbonding_buffer": | "unbonding_period": | "Decimal": | "PoolConfigUpdateRequest": | "properties": | "active": | "protocol_fee_contract": | "protocol_fee_percent": | { | { | "reward_contract": | "Uint128":;