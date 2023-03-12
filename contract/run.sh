//lunch

near call $crow lunch_campaign '{"time_start":"1678437300", "time_end": "1678437600", "goal":"50", "name_campaign":"Ung ho nguoi ngheo"}' --accountId near1.tranchinhwalletnear.testnet --amount 5

near call $crow donate '{"id_campaign": "0", "amount":"3"}' --accountId $near2 --amount 5

near call $crow un_donate '{"id_campaign":"1", "amount":"5"}' --accountId $near2 --amount