{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "QueryStateResponse",
  "type": "object",
  "required": [
    "state"
  ],
type "properties": = { 
properties": : { 
type "state": = { 
state": : { 
      "$ref": "#/definitions/State"
    }
  
}};

type "definitions": = { 
definitions": : { 
type "State": = { 
_state": : { 
      "type": "object",
      "required": [
        "next_pool_id"
      ],
type "properties": = { 
properties": : { 
type "next_pool_id": = { 
next_pool_id": : { 
          "type": "integer",
          "format": "uint64",
          "minimum": 0.0
        }
      }
    }
  }
}

export type Query_state_response.json = "properties": | "state": | "definitions": | "State": | "properties": | "next_pool_id":;