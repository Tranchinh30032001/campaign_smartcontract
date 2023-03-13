use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, Vector};
use near_sdk::json_types::{U128, U64};
use near_sdk::{env, near_bindgen, require, AccountId, BorshStorageKey, Promise};
mod campaign;
mod campaign_cancel;
mod util;
use campaign::*;
use campaign_cancel::*;
use util::*;

pub type IdCampaign = u64;
const ERR_TOTAL_SUPPLY_OVERFLOW: &str = "Total supply overflow";

#[derive(BorshSerialize, BorshStorageKey)]
pub enum StorageKey {
    DetailCampaign,
    Contributors,
    ContributorsNested,
    ListCampaign,
    ListCampaignSuccess,
    ListCampaignCancel,
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct Contract {
    count_campaign: u64, // đếm số lượng campaign đã tạo
    id_index: u64,       // id của mỗi campaign
    campaign: LookupMap<IdCampaign, Campaign>,
    // IDCampaing => AccountId => amount
    contributors: LookupMap<IdCampaign, LookupMap<AccountId, u128>>,
    list_campaign: Vector<String>, // danh sach cac campaign da khoi tao - danh sach campaign da bi cancel
    list_campaign_success: Vector<String>,
    list_campaign_cancel: Vector<CampaignCancel>,
}

impl Default for Contract {
    fn default() -> Self {
        Self {
            count_campaign: 0,
            id_index: 0,
            campaign: LookupMap::new(StorageKey::DetailCampaign),
            contributors: LookupMap::new(StorageKey::Contributors),
            list_campaign: Vector::new(StorageKey::ListCampaign),
            list_campaign_success: Vector::new(StorageKey::ListCampaignSuccess),
            list_campaign_cancel: Vector::new(StorageKey::ListCampaignCancel),
        }
    }
}

#[near_bindgen]
impl Contract {
    #[payable]
    pub fn lunch_campaign(
        &mut self,
        time_start: U64,
        time_end: U64,
        goal: U128,
        name_campaign: String,
    ) -> IdCampaign {
        // phí khởi tạo 1 campaign là 1 near.
        assert_at_least_fee_initial_campaign();
        let init_storage = env::storage_usage();
        let time_start = time_start.0;
        let time_end = time_end.0;
        if time_start >= time_end {
            env::panic_str("Time start must lower than Time end");
        }
        let goal = goal.0;
        let campaign = Campaign {
            id: self.id_index,
            name_campaign,
            creator: env::signer_account_id(),
            goal,
            /// chi tieu
            amount: 0, //
            time_start,
            time_end,
            finished: false,
            refund: false,
        };
        self.campaign.insert(&campaign.id, &campaign);
        self.list_campaign.push(&campaign.name_campaign);
        self.count_campaign += 1;
        self.id_index += 1;
        refund_deposit(init_storage);
        return campaign.id;
    }

    pub fn check_campaign(&self, id_campaign: IdCampaign) -> bool {
        match self.campaign.get(&id_campaign) {
            Some(_x) => true,
            _ => false,
        }
    }

    #[payable]
    pub fn cancel_campaign(&mut self, id_campaign: IdCampaign) -> bool {
        assert_at_least_one_yocto();
        let init_storage = env::storage_usage();
        if !self.check_campaign(id_campaign) {
            env::panic_str("This campaign doesn't exsit");
        }
        if self.campaign.get(&id_campaign).unwrap().creator != env::predecessor_account_id() {
            env::panic_str("Just the creator can execute this function");
        }
        let campaign_cancel = CampaignCancel {
            name_campaign: self.campaign.get(&id_campaign).unwrap().name_campaign,
            time_cancel: env::block_timestamp_ms(),
            canceler: env::predecessor_account_id(),
        };
        self.list_campaign_cancel.push(&campaign_cancel);
        self.count_campaign = self
            .count_campaign
            .checked_sub(1)
            .unwrap_or_else(|| env::panic_str(ERR_TOTAL_SUPPLY_OVERFLOW));
        self.campaign.remove(&id_campaign);
        self.list_campaign.swap_remove(id_campaign);
        refund_deposit(init_storage);
        return true;
    }

    #[payable]
    pub fn donate(&mut self, id_campaign: IdCampaign, amount: U128) {
        assert_at_least_one_yocto();
        // let deposit_attached = env::attached_deposit();
        let deposit_attached = amount.0;
        if !self.check_campaign(id_campaign) {
            env::panic_str("This campaign doesn't exsit");
        }
        let time_start = self.campaign.get(&id_campaign).unwrap().time_start;
        let time_end = self.campaign.get(&id_campaign).unwrap().time_end;
        require!(
            env::block_timestamp_ms() >= time_start,
            "This campaign not start yet"
        );
        require!(
            env::block_timestamp_ms() <= time_end,
            "this campaign has end"
        );
        // update amount of campaign
        let mut old_campaign = self.campaign.get(&id_campaign).unwrap();
        old_campaign.amount = old_campaign
            .amount
            .checked_add(deposit_attached)
            .unwrap_or_else(|| env::panic_str(ERR_TOTAL_SUPPLY_OVERFLOW));
        self.campaign.insert(&id_campaign, &old_campaign);

        //update contributors
        if let Some(mut result) = self.contributors.get(&id_campaign) {
            let mut money = result.get(&env::predecessor_account_id()).unwrap();
            money = money
                .checked_add(deposit_attached)
                .unwrap_or_else(|| env::panic_str(ERR_TOTAL_SUPPLY_OVERFLOW));
            result.insert(&env::predecessor_account_id(), &money);
        } else {
            let mut detail: LookupMap<AccountId, u128> =
                LookupMap::new(StorageKey::ContributorsNested);
            detail.insert(&env::predecessor_account_id(), &deposit_attached);
            self.contributors.insert(&id_campaign, &detail);
        }
    }

    #[payable]
    pub fn un_donate(&mut self, id_campaign: IdCampaign, amount: U128) {
        assert_at_least_one_yocto();
        // let amount = amount.0 * 1_000_000_000_000_000_000_000_000;
        let amount = amount.0;
        let init_storage = env::storage_usage();
        if !self.check_campaign(id_campaign) {
            env::panic_str("This campaign doesn't exsit");
        }
        // kiểm tra xem user đã từng donate trước đây chưa, nếu chưa return;
        require!(
            self.check_donated(id_campaign),
            "You haven't donate this campaign before"
        );
        let amount_donated = self
            .contributors
            .get(&id_campaign)
            .unwrap()
            .get(&env::predecessor_account_id())
            .unwrap();
        let refund = std::cmp::min(amount, amount_donated);
        Promise::new(env::predecessor_account_id()).transfer(refund);

        //update campaign
        let mut old_campaign = self.campaign.get(&id_campaign).unwrap();
        old_campaign.amount = old_campaign
            .amount
            .checked_sub(refund)
            .unwrap_or_else(|| env::panic_str(ERR_TOTAL_SUPPLY_OVERFLOW));
        self.campaign.insert(&id_campaign, &old_campaign);

        // update contributor
        let mut amount_contributor = self
            .contributors
            .get(&id_campaign)
            .unwrap()
            .get(&env::predecessor_account_id())
            .unwrap();
        amount_contributor = amount_contributor
            .checked_sub(refund)
            .unwrap_or_else(|| env::panic_str(ERR_TOTAL_SUPPLY_OVERFLOW));
        self.contributors
            .get(&id_campaign)
            .unwrap()
            .insert(&env::predecessor_account_id(), &amount_contributor);
        refund_deposit(init_storage);
    }

    #[payable]
    pub fn finished_campaign(&mut self, id_campaign: IdCampaign) {
        let init_storage = env::storage_usage();
        let mut campaign = self.campaign.get(&id_campaign).unwrap();
        let time_end = campaign.time_end;
        let creator = campaign.creator.clone();
        let finished = campaign.finished;
        let goal = campaign.goal;
        let amount = campaign.amount;
        require!(
            env::block_timestamp_ms() >= time_end,
            "The time of this campaign is not over yet"
        );
        require!(
            env::predecessor_account_id() == creator,
            "You are not the creator of this campaign"
        );
        if finished {
            panic!("This campaign was finished");
        }
        if amount >= goal {
            self.list_campaign_success.push(&campaign.name_campaign);
            campaign.amount = 0;
            self.campaign.insert(&id_campaign, &campaign);
            Promise::new(creator).transfer(amount);
        } else {
            campaign.refund = true;
            self.campaign.insert(&id_campaign, &campaign);
        }
        campaign.finished = true;
        self.campaign.insert(&id_campaign, &campaign);
        refund_deposit(init_storage);
    }

    #[payable]
    pub fn refund(&mut self, id_campaign: IdCampaign) {
        let init_storage = env::storage_usage();
        let mut campaign = self.campaign.get(&id_campaign).unwrap();
        require!(campaign.refund, "This campaign can't not refund");
        let mut contributor = self.contributors.get(&id_campaign).unwrap();

        // Trường hợp user đã donate và rút lại
        if let Some(res) = contributor.get(&env::predecessor_account_id()) {
            Promise::new(env::predecessor_account_id()).transfer(res);
            //remove out of contributors
            contributor.remove(&env::predecessor_account_id());
            campaign.amount -= res;
            //update amount campaign
            self.campaign.insert(&id_campaign, &campaign);
        } else {
            env::panic_str("You never donate this campaign");
        }
        refund_deposit(init_storage);
    }

    pub fn get_amount_donated(&self, id_campaign: IdCampaign) -> u128 {
        let x = match self
            .contributors
            .get(&id_campaign)
            .unwrap()
            .get(&env::predecessor_account_id())
        {
            Some(res) => Some(res),
            _ => None,
        };
        return x.unwrap();
    }

    fn check_donated(&self, id_campaign: IdCampaign) -> bool {
        if let Some(res) = self
            .contributors
            .get(&id_campaign)
            .unwrap()
            .get(&env::predecessor_account_id())
        {
            if res > 0 {
                return true;
            }
            return false;
        }
        return false;
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use near_sdk::test_utils::{accounts, VMContextBuilder};
    use near_sdk::{testing_env, AccountId, VMContext};

    fn get_context(is_view: bool, signer: AccountId) -> VMContext {
        let mut builder = VMContextBuilder::new();
        builder
            .current_account_id(accounts(0))
            .signer_account_id(signer.clone())
            .predecessor_account_id(signer)
            .is_view(is_view)
            .block_timestamp(100)
            .storage_usage(100000);
        builder.build()
    }

    #[test]
    fn init_default_contract_test() {
        let context = get_context(false, accounts(0));
        testing_env!(context);
        let contract = Contract::default();
        assert_eq!(contract.count_campaign, 0, "Id_index must equa zero");
        assert_eq!(contract.count_campaign, 0, "count_campaign must equa zero");
        assert_eq!(
            contract.list_campaign.len(),
            0,
            "list_campaign must equa zero"
        );
    }

    fn init_lunch_campaign(signer: AccountId) {
        let mut context = get_context(false, signer);
        context.attached_deposit = 2 * 10u128.pow(24);
        testing_env!(context);
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(100);
        let goal = U128::from(100 * 10u128.pow(24)); //100near
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
    }

    #[test]
    fn test_lunch_campaign() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 1 * 10u128.pow(24);
        testing_env!(context);
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(100);
        let goal = U128::from(100 * 10u128.pow(24)); //100near
        let name_campaign = String::from("Khoi Nghiep");
        let result = contract.lunch_campaign(time_start, time_end, goal, name_campaign);

        let compare_campaign = contract.campaign.get(&0).unwrap();
        assert_eq!(result, 0);
        assert_eq!(compare_campaign.name_campaign, "Khoi Nghiep".to_string());
        assert_eq!(compare_campaign.goal, 100 * 10u128.pow(24));
        assert_eq!(compare_campaign.amount, 0);
        assert_eq!(contract.count_campaign, 1);
        assert_eq!(contract.id_index, 1);
    }

    #[test]
    fn test_check_campaign() {
        let context = get_context(false, accounts(0));
        testing_env!(context);
        let contract = Contract::default();
        test_lunch_campaign();
        assert_eq!(
            contract.check_campaign(0),
            true,
            "This Id not initialized yet"
        );
    }

    #[test]
    #[should_panic(expected = "This campaign doesn't exsit")]
    fn test_cancel_campaign_id() {
        let mut contract = Contract::default();
        test_lunch_campaign();
        assert_ne!(contract.cancel_campaign(3), true);
    }

    #[test]
    #[should_panic(expected = "Just the creator can execute this function")]
    fn test_cancel_campaign_creator() {
        let mut contract = Contract::default();
        init_lunch_campaign(accounts(0));
        let mut context = get_context(false, accounts(5));
        context.attached_deposit = 1000;
        testing_env!(context);
        assert_ne!(contract.cancel_campaign(0), true);
    }

    #[test]
    fn test_cancel_success() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 2 * 10u128.pow(24);
        testing_env!(context);
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(100);
        let goal = U128::from(100 * 10u128.pow(24)); //100near
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        assert_eq!(contract.list_campaign.len(), 1);
        assert_eq!(contract.cancel_campaign(0), true);
    }
    #[test]
    #[should_panic(expected = "This campaign doesn't exsit")]
    fn test_donate_not_exsit() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 2 * 10u128.pow(24);
        testing_env!(context);
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(100);
        let goal = U128::from(100 * 10u128.pow(24)); //100near
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        contract.donate(2, U128::from(10));
    }

    #[test]
    #[should_panic(expected = "This campaign not start yet")]
    fn test_donate_fail_not_start() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 2 * 10u128.pow(24);
        context.block_timestamp = 0;
        testing_env!(context);
        let mut contract = Contract::default();
        let time_start = U64::from(10);
        let time_end = U64::from(100);
        let goal = U128::from(100 * 10u128.pow(24)); //100near
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        contract.donate(0, U128::from(10))
    }

    #[test]
    #[should_panic(expected = "this campaign has end")]
    fn test_donate_fail_not_end() {
        let mut context = get_context(false, accounts(0));
        context.block_timestamp = 1_000_000_000;
        context.attached_deposit = 2 * 10u128.pow(24);
        testing_env!(context);
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(100);
        let goal = U128::from(100 * 10u128.pow(24)); //100near
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        contract.donate(0, U128::from(10))
    }
    #[test]
    #[should_panic(expected = "Total supply overflow")]
    fn test_donate_overflow() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 2 * 10u128.pow(24);
        testing_env!(context);
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(1000);
        let goal = U128::from(100 * 10u128.pow(24)); //100near
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        contract.donate(0, U128::from(10));
        contract.donate(0, U128::from(340282366920938463463374607431768211455));
    }

    #[test]
    fn test_donate_success() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 2 * 10u128.pow(24);
        testing_env!(context);
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(1000);
        let goal = U128::from(100 * 10u128.pow(24)); //100near
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        contract.donate(0, U128::from(10));
        assert_eq!(contract.campaign.get(&0).unwrap().amount, 10);
        assert_eq!(
            contract
                .contributors
                .get(&0)
                .unwrap()
                .get(&env::predecessor_account_id())
                .unwrap(),
            10
        );
    }
    #[test]
    #[should_panic(expected = "You haven't donate this campaign before")]
    fn test_un_donate_failed() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 2 * 10u128.pow(24);
        testing_env!(context.clone());
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(1000);
        let goal = U128::from(100 * 10u128.pow(24)); //100near
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        contract.donate(0, U128::from(10));
        context.predecessor_account_id = accounts(1);
        testing_env!(context);
        contract.un_donate(0, U128(5));
    }

    #[test]
    fn test_un_donate_success() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 2 * 10u128.pow(24);
        testing_env!(context);
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(1000);
        let goal = U128::from(100 * 10u128.pow(24)); //100near
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        contract.donate(0, U128::from(10));
        contract.un_donate(0, U128::from(10));
        assert_eq!(contract.campaign.get(&0).unwrap().amount, 0);
        assert_eq!(
            contract
                .contributors
                .get(&0)
                .unwrap()
                .get(&accounts(0))
                .unwrap(),
            0
        );
    }

    #[test]
    #[should_panic(expected = "The time of this campaign is not over yet")]
    fn test_finished_campaign_not_end() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 2 * 10u128.pow(24);
        context.block_timestamp = 1_000_000;
        testing_env!(context);
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(100);
        let goal = U128::from(100 * 10u128.pow(24)); //100near
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        contract.finished_campaign(0);
    }

    #[test]
    #[should_panic(expected = "You are not the creator of this campaign")]
    fn test_finished_campaign_not_creator() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 2 * 10u128.pow(24);
        context.block_timestamp = 1_000_000_000;
        testing_env!(context.clone());
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(100);
        let goal = U128::from(100 * 10u128.pow(24)); //100near
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        context.predecessor_account_id = accounts(2);
        context.attached_deposit = 1 * 10u128.pow(12);
        testing_env!(context);
        contract.finished_campaign(0);
    }

    #[test]
    // #[ignore]
    #[should_panic(expected = "This campaign was finished")]
    fn test_finished_campaign_finished() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 2 * 10u128.pow(24);
        testing_env!(context.clone());
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(100);
        let goal = U128::from(100 * 10u128.pow(24)); //100near
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        context.block_timestamp = 1_000_000_000;
        testing_env!(context);
        contract.finished_campaign(0);

        assert_eq!(contract.campaign.get(&0).unwrap().finished, true);
        contract.finished_campaign(0);
        contract.finished_campaign(0);
    }

    #[test]
    // #[ignore] // test truong hop amount >= goal
    fn test_finished_campaign_ok() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 2 * 10u128.pow(24);
        testing_env!(context.clone());
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(100);
        let goal = U128::from(100);
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        contract.donate(0, U128(200));
        context.block_timestamp = 1_000_000_000;
        testing_env!(context);
        contract.finished_campaign(0);
        assert_eq!(contract.list_campaign_success.len(), 1);
        assert_eq!(contract.campaign.get(&0).unwrap().amount, 0);
        assert_eq!(contract.campaign.get(&0).unwrap().finished, true);
        assert_eq!(contract.campaign.get(&0).unwrap().refund, false);
    }

    #[test] // test truong hop amount < goal
    fn test_finished_campaign_not_ok() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 2 * 10u128.pow(24);
        testing_env!(context.clone());
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(100);
        let goal = U128::from(100);
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        contract.donate(0, U128(90));
        context.block_timestamp = 1_000_000_000;
        testing_env!(context);
        contract.finished_campaign(0);
        assert_eq!(contract.list_campaign_success.len(), 0);
        assert_eq!(contract.campaign.get(&0).unwrap().amount, 90);
        assert_eq!(contract.campaign.get(&0).unwrap().finished, true);
        assert_eq!(contract.campaign.get(&0).unwrap().refund, true);
    }

    #[test]
    #[should_panic(expected = "This campaign can't not refund")]
    fn test_refund_not_refund() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 2 * 10u128.pow(24);
        testing_env!(context.clone());
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(100);
        let goal = U128::from(100);
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        contract.donate(0, U128(200));
        context.block_timestamp = 1_000_000_000;
        testing_env!(context);
        contract.finished_campaign(0);
        contract.refund(0);
    }
    #[test]
    #[should_panic(expected = "You never donate this campaign")]
    fn test_refund_not_exsit() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 2 * 10u128.pow(24);
        testing_env!(context.clone());
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(100);
        let goal = U128::from(100);
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        contract.donate(0, U128(50));
        context.block_timestamp = 1_000_000_000;
        testing_env!(context.clone());
        contract.finished_campaign(0);
        context.predecessor_account_id = accounts(3);
        testing_env!(context);
        contract.refund(0);
    }

    #[test]
    fn test_refund_success() {
        let mut context = get_context(false, accounts(0));
        context.attached_deposit = 2 * 10u128.pow(24);
        testing_env!(context.clone());
        let mut contract = Contract::default();
        let time_start = U64::from(0);
        let time_end = U64::from(100);
        let goal = U128::from(100);
        let name_campaign = String::from("Khoi Nghiep");
        contract.lunch_campaign(time_start, time_end, goal, name_campaign);
        contract.donate(0, U128(50));
        context.block_timestamp = 1_000_000_000;
        testing_env!(context);
        contract.finished_campaign(0);
        contract.refund(0);
        assert_eq!(contract.campaign.get(&0).unwrap().amount, 0);
        assert_eq!(
            contract
                .contributors
                .get(&0)
                .unwrap()
                .contains_key(&accounts(0)),
            false
        );
    }
}
