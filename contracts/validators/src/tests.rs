#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{instantiate, query};
    use crate::msg::{GetStateResponse, InstantiateMsg, QueryMsg};
    use crate::state::State;
    use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
    use cosmwasm_std::{coins, from_binary, Addr};

    #[test]
    fn proper_initialization() {
        let mut deps = mock_dependencies(&[]);

        let msg = InstantiateMsg {
            vault_denom: "utest".to_string(),
            pools_contract_addr: Addr::unchecked("pools_address"),
        };
        let expected_state = State {
            manager: Addr::unchecked("creator"),
            vault_denom: "utest".to_string(),
            pools_contract_addr: Addr::unchecked("pools_address"),
        };
        let info = mock_info("creator", &coins(1000, "earth"));

        // we can just call .unwrap() to assert this was a success
        let res = instantiate(deps.as_mut(), mock_env(), info, msg).unwrap();
        assert_eq!(0, res.messages.len());

        // it worked, let's query the state
        let res = query(deps.as_ref(), mock_env(), QueryMsg::GetState {}).unwrap();
        let value: GetStateResponse = from_binary(&res).unwrap();
        assert_eq!(value.state, expected_state);
    }
}
