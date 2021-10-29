{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "QueryBatchUndelegationResponse",
  "type": "object",
type "properties": = { 
properties": : { 
type "batch": = { 
batch": : { 
      "anyOf": [
type { = { 
 : { 
          "$ref": "#/definitions/BatchUndelegationRecord"
        
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
type "BatchUndelegationRecord": = { 
_batch_undelegation_record": : { 
      "type": "object",
      "required": [
        "create_time",
        "last_updated_slashing_pointer",
        "prorated_amount",
        "reconciled",
        "unbonding_slashing_ratio",
        "undelegated_amount"
      ],
type "properties": = { 
properties": : { 
type "create_time": = { 
create_time": : { 
          "$ref": "#/definitions/Timestamp"
        
}};

type "est_release_time": = { 
est_release_time": : { 
          "anyOf": [
type { = { 
 : { 
              "$ref": "#/definitions/Timestamp"
            
}};

type { = { 
 : { 
              "type": "null"
            }
          ]
        
}};

type "last_updated_slashing_pointer": = { 
last_updated_slashing_pointer": : { 
          "$ref": "#/definitions/Decimal"
        
}};

type "prorated_amount": = { 
prorated_amount": : { 
          "$ref": "#/definitions/Decimal"
        
}};

type "reconciled": = { 
reconciled": : { 
          "type": "boolean"
        
}};

type "unbonding_slashing_ratio": = { 
unbonding_slashing_ratio": : { 
          "$ref": "#/definitions/Decimal"
        
}};

type "undelegated_amount": = { 
undelegated_amount": : { 
          "$ref": "#/definitions/Uint128"
        }
      }
    
}};

type "Decimal": = { 
_decimal": : { 
      "description": "A fixed-point decimal value with 18 fractional digits, i.e. Decimal(1_000_000_000_000_000_000) == 1.0\n\nThe greatest possible value that can be represented is 340282366920938463463.374607431768211455 (which is (2^128 - 1) / 10^18)",
      "type": "string"
    
}};

type "Timestamp": = { 
_timestamp": : { 
      "description": "A point in time in nanosecond precision.\n\nThis type can represent times from 1970-01-01T00:00:00Z to 2554-07-21T23:34:33Z.\n\n## Examples\n\n``` # use cosmwasm_std::Timestamp; let ts = Timestamp::from_nanos(1_000_000_202); assert_eq!(ts.nanos(), 1_000_000_202); assert_eq!(ts.seconds(), 1); assert_eq!(ts.subsec_nanos(), 202);\n\nlet ts = ts.plus_seconds(2); assert_eq!(ts.nanos(), 3_000_000_202); assert_eq!(ts.seconds(), 3); assert_eq!(ts.subsec_nanos(), 202); ```",
      "allOf": [
type { = { 
 : { 
          "$ref": "#/definitions/Uint64"
        }
      ]
    
}};

type "Uint128": = { 
_uint128": : { 
      "description": "A thin wrapper around u128 that is using strings for JSON encoding/decoding, such that the full u128 range can be used for clients that convert JSON numbers to floats, like JavaScript and jq.\n\n# Examples\n\nUse `from` to create instances of this and `u128` to get the value out:\n\n``` # use cosmwasm_std::Uint128; let a = Uint128::from(123u128); assert_eq!(a.u128(), 123);\n\nlet b = Uint128::from(42u64); assert_eq!(b.u128(), 42);\n\nlet c = Uint128::from(70u32); assert_eq!(c.u128(), 70); ```",
      "type": "string"
    
}};

type "Uint64": = { 
_uint64": : { 
      "description": "A thin wrapper around u64 that is using strings for JSON encoding/decoding, such that the full u64 range can be used for clients that convert JSON numbers to floats, like JavaScript and jq.\n\n# Examples\n\nUse `from` to create instances of this and `u64` to get the value out:\n\n``` # use cosmwasm_std::Uint64; let a = Uint64::from(42u64); assert_eq!(a.u64(), 42);\n\nlet b = Uint64::from(70u32); assert_eq!(b.u64(), 70); ```",
      "type": "string"
    }
  }
}

export type Query_batch_undelegation_response.json = "properties": | "batch": | { | { | "definitions": | "BatchUndelegationRecord": | "properties": | "create_time": | "est_release_time": | { | { | "last_updated_slashing_pointer": | "prorated_amount": | "reconciled": | "unbonding_slashing_ratio": | "undelegated_amount": | "Decimal": | "Timestamp": | { | "Uint128": | "Uint64":;