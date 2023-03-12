use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
    AccountId,
};
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct Campaign {
    pub id: u64,
    pub name_campaign: String,
    pub creator: AccountId,
    pub goal: u128,
    pub amount: u128,
    pub time_start: u64,
    pub time_end: u64,
    pub finished: bool,
    pub refund: bool,
}
