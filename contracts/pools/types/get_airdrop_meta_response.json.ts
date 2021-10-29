{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "GetAirdropMetaResponse",
  "type": "object",
type "properties": = { 
properties": : { 
type "airdrop_meta": = { 
airdrop_meta": : { 
      "anyOf": [
type { = { 
 : { 
          "$ref": "#/definitions/AirdropRegistryInfo"
        
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

type "AirdropRegistryInfo": = { 
_airdrop_registry_info": : { 
      "type": "object",
      "required": [
        "airdrop_contract",
        "cw20_contract"
      ],
type "properties": = { 
properties": : { 
type "airdrop_contract": = { 
airdrop_contract": : { 
          "$ref": "#/definitions/Addr"
        
}};

type "cw20_contract": = { 
cw20_contract": : { 
          "$ref": "#/definitions/Addr"
        }
      }
    }
  }
}

export type Get_airdrop_meta_response.json = "properties": | "airdrop_meta": | { | { | "definitions": | "Addr": | "AirdropRegistryInfo": | "properties": | "airdrop_contract": | "cw20_contract":;