// Copyright 2024 RISC Zero, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use alloy::{
    network::EthereumWallet, node_bindings::Anvil, primitives::U256, providers::ProviderBuilder,
    signers::local::PrivateKeySigner,
};
use alloy_sol_types::sol;
use blobstream0_core::{post_batch, prove_block_range};
use blobstream0_primitives::IBlobstream::{self, BinaryMerkleProof, DataRootTuple};
use reqwest::header;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as, DisplayFromStr};
use std::sync::Arc;
use tendermint_rpc::{Client, HttpClient};

sol!(
    #[sol(rpc)]
    MockVerifier,
    "../contracts/out/RiscZeroMockVerifier.sol/RiscZeroMockVerifier.json"
);

const CELESTIA_RPC_URL: &str = "https://celestia-testnet.brightlystake.com";

const BATCH_START: u32 = 2768370;
const BATCH_END: u32 = 2768400;
const PROOF_HEIGHT: u32 = 2768375;

/// Type matches Celestia API endpoint for generating proof.
/// https://docs.celestia.org/developers/blobstream-proof-queries#_1-data-root-inclusion-proof
#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
struct DataRootInclusionResponse {
    #[serde_as(as = "DisplayFromStr")]
    total: u64,
    #[serde_as(as = "DisplayFromStr")]
    index: u64,
    #[serde_as(as = "Base64")]
    leaf_hash: Vec<u8>,
    #[serde_as(as = "Vec<Base64>")]
    aunts: Vec<Vec<u8>>,
}

#[tokio::test(flavor = "multi_thread")]
async fn e2e_basic_range() -> anyhow::Result<()> {
    // Set dev mode for test.
    std::env::set_var("RISC0_DEV_MODE", "true");

    // Spin up a local Anvil node.
    // Ensure `anvil` is available in $PATH.
    let anvil = Anvil::new().try_spawn()?;

    // Set up signer from the first default Anvil account (Alice).
    let signer: PrivateKeySigner = anvil.keys()[0].clone().into();
    let wallet = EthereumWallet::from(signer);

    // Create a provider with the wallet.
    let rpc_url = anvil.endpoint().parse()?;
    let provider = ProviderBuilder::new()
        .with_recommended_fillers()
        .wallet(wallet)
        .on_http(rpc_url);

    let verifier = MockVerifier::deploy(&provider, [0, 0, 0, 0].into()).await?;

    let tm_client = Arc::new(HttpClient::new(CELESTIA_RPC_URL)?);
    let trusted_block_hash = tm_client
        .header(BATCH_START - 1)
        .await?
        .header
        .hash()
        .as_bytes()
        .try_into()
        .unwrap();

    // Deploy the contract.
    let contract = IBlobstream::deploy(
        &provider,
        anvil.addresses()[0],
        verifier.address().clone(),
        // Uses Celestia block hash at height below proving range on Mocha
        trusted_block_hash,
        BATCH_START as u64 - 1,
    )
    .await?;

    let receipt =
        prove_block_range(tm_client.clone(), BATCH_START as u64..BATCH_END as u64).await?;

    post_batch(&contract, &receipt).await?;

    let height = contract.latestHeight().call().await?;
    assert_eq!(height._0, BATCH_END as u64 - 1);

    // Somewhat hacky to do this manually, seems no Rust tooling for this endpoint.
    let http_client = reqwest::Client::new();
    let proof_response = http_client
        .get(format!(
            "{}/data_root_inclusion_proof?height={}&start={}&end={}",
            CELESTIA_RPC_URL, PROOF_HEIGHT, BATCH_START, BATCH_END
        ))
        .header(header::CONTENT_TYPE, "application/json")
        .send()
        .await?;

    let response = proof_response.json::<serde_json::Value>().await?;
    let response: DataRootInclusionResponse =
        serde_json::from_value(response["result"]["proof"].clone())?;

    let proof_data_root = tm_client
        .header(PROOF_HEIGHT)
        .await?
        .header
        .data_hash
        .unwrap()
        .as_bytes()
        .try_into()
        .unwrap();
    // Validate data root inclusion.
    let is_valid = contract
        .verifyAttestation(
            U256::from(1),
            DataRootTuple {
                height: U256::from(PROOF_HEIGHT),
                // TODO this is fixed from on chain, but could be pulled from node to be dynamic
                dataRoot: proof_data_root,
            },
            BinaryMerkleProof {
                sideNodes: response
                    .aunts
                    .iter()
                    .map(|node| node.as_slice().try_into().unwrap())
                    .collect(),
                key: U256::from(response.index),
                numLeaves: U256::from(response.total),
            },
        )
        .call()
        .await?;
    assert!(is_valid._0);

    Ok(())
}
