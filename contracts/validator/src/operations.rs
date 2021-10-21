// SEND EVENTS FROM THIS CONTRACT
pub const EVENT_REDELEGATE_ID: u64 = 0;
pub const EVENT_REDELEGATE_TYPE: &str = &"stader_redelegate";
pub const EVENT_REDELEGATE_KEY_SRC_ADDR: &str = &"src";
pub const EVENT_REDELEGATE_KEY_DST_ADDR: &str = &"dst";

// RECEIVE EVENTS FROM OTHER CONTRACTS
pub const MESSAGE_REPLY_REWARD_INST_ID: u64 = 1;
pub const EVENT_INSTANTIATE_TYPE: &str = &"stader_reward_instantiate";
pub const EVENT_INSTANTIATE_KEY_CONTRACT_ADDR: &str = &"reward_contract_address";
