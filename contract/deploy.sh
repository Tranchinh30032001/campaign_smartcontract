#!/bin/sh

./build.sh

if [ $? -ne 0 ]; then
  echo ">> Error building contract"
  exit 1
fi

echo ">> Deploying contract"
near delete crowdfund.tranchinhwalletnear.testnet tranchinhwalletnear.testnet

near create-account crowdfund.tranchinhwalletnear.testnet --masterAccount tranchinhwalletnear.testnet --initial-balance 10

# https://docs.near.org/tools/near-cli#near-dev-deploy
near deploy crowdfund.tranchinhwalletnear.testnet --wasmFile ./target/wasm32-unknown-unknown/release/hello_near.wasm
