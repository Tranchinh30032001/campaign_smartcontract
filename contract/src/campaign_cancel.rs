use near_sdk::{
    borsh::{self, BorshDeserialize, BorshSerialize},
    serde::{Deserialize, Serialize},
    AccountId,
};
#[derive(BorshDeserialize, BorshSerialize, Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]

pub struct CampaignCancel {
    pub name_campaign: String,
    pub time_cancel: u64,
    pub canceler: AccountId,
}
