{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "QueryMsg",
  "oneOf": [
type { = { 
 : { 
      "type": "object",
      "required": [
        "config"
      ],
type "properties": = { 
properties": : { 
type "config": = { 
config": : { 
          "type": "object"
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "state"
      ],
type "properties": = { 
properties": : { 
type "state": = { 
state": : { 
          "type": "object"
        }
      
}};

      "additionalProperties": false
    
}};

type { = { 
 : { 
      "type": "object",
      "required": [
        "pool"
      ],
type "properties": = { 
properties": : { 
type "pool": = { 
pool": : { 
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
        "batch_undelegation"
      ],
type "properties": = { 
properties": : { 
type "batch_undelegation": = { 
batch_undelegation": : { 
          "type": "object",
          "required": [
            "batch_id",
            "pool_id"
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
            }
          }
        }
      
}};

      "additionalProperties": false
    }
  ]
}

export type Query_msg.json = { | "properties": | "config": | { | "properties": | "state": | { | "properties": | "pool": | "properties": | "pool_id": | { | "properties": | "batch_undelegation": | "properties": | "batch_id": | "pool_id":;