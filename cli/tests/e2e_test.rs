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

use alloy::network::Ethereum;
use alloy::providers::Provider;
use alloy::{
    network::EthereumWallet,
    node_bindings::{Anvil, AnvilInstance},
    primitives::U256,
    providers::ProviderBuilder,
    signers::local::PrivateKeySigner,
};
use alloy_sol_types::{sol, SolCall};
use blobstream0_core::{post_batch, prove_block_range};
use blobstream0_primitives::IBlobstream::{
    self, BinaryMerkleProof, DataRootTuple, IBlobstreamInstance,
};
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
sol!(
    #[sol(rpc)]
    ERC1967Proxy,
    "../contracts/out/ERC1967Proxy.sol/ERC1967Proxy.json"
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

async fn setup_test_environment() -> anyhow::Result<(
    AnvilInstance,
    IBlobstreamInstance<impl Provider<Ethereum>, Ethereum>,
)> {
    // Set dev mode for test.
    std::env::set_var("RISC0_DEV_MODE", "true");

    // Spin up a local Anvil node.
    let anvil = Anvil::new().try_spawn()?;

    // Set up signer from the first default Anvil account (Alice).
    let signer: PrivateKeySigner = anvil.keys()[0].clone().into();
    let wallet = EthereumWallet::from(signer);

    // Create a provider with the wallet.
    let rpc_url = anvil.endpoint().parse()?;
    let provider = ProviderBuilder::new().wallet(wallet).connect_http(rpc_url);

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
    let implementation = IBlobstream::deploy(&provider).await?;
    tracing::debug!(target: "blobstream0::cli", "Deployed implementation contract");

    let contract = ERC1967Proxy::deploy(
        &provider,
        implementation.address().clone(),
        IBlobstream::initializeCall {
            _admin: anvil.addresses()[0],
            _verifier: verifier.address().clone(),
            _trustedHash: trusted_block_hash,
            _trustedHeight: BATCH_START as u64 - 1,
            _minBatchSize: 0,
        }
        .abi_encode()
        .into(),
    )
    .await?;
    // Pretend as if the proxy is the contract itself, requests forwarded to implementation.
    let contract = IBlobstream::new(contract.address().clone(), provider);

    Ok((anvil, contract))
}

#[tokio::test(flavor = "multi_thread")]
async fn e2e_basic_range() -> anyhow::Result<()> {
    let (_anvil, contract) = setup_test_environment().await?;

    let tm_client = Arc::new(HttpClient::new(CELESTIA_RPC_URL)?);
    let receipt =
        prove_block_range(tm_client.clone(), BATCH_START as u64..BATCH_END as u64).await?;

    post_batch(&contract, &receipt).await?;

    let height = contract.latestHeight().call().await?;
    assert_eq!(height, BATCH_END as u64 - 1);

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
    assert!(is_valid);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_admin_functions() -> anyhow::Result<()> {
    let (_anvil, contract) = setup_test_environment().await?;

    // Test adminSetImageId
    let new_image_id = [1u8; 32];
    contract
        .adminSetImageId(new_image_id.into())
        .send()
        .await?
        .watch()
        .await?;
    let current_image_id = contract.imageId().call().await?;
    assert_eq!(current_image_id, new_image_id);

    // Test adminSetVerifier
    let new_verifier = MockVerifier::deploy(contract.provider(), [1, 1, 1, 1].into()).await?;
    contract
        .adminSetVerifier(new_verifier.address().clone())
        .send()
        .await?
        .watch()
        .await?;
    let current_verifier = contract.verifier().call().await?;
    assert_eq!(current_verifier, *new_verifier.address());

    // Test adminSetTrustedState
    let new_trusted_hash = [2u8; 32];
    let new_trusted_height = 100u64;
    contract
        .adminSetTrustedState(new_trusted_hash.into(), new_trusted_height.into())
        .send()
        .await?
        .watch()
        .await?;
    let current_trusted_hash = contract.latestBlockHash().call().await?;
    let current_trusted_height = contract.latestHeight().call().await?;
    assert_eq!(current_trusted_hash, new_trusted_hash);
    assert_eq!(current_trusted_height, new_trusted_height);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_contract_upgrade() -> anyhow::Result<()> {
    let (_anvil, contract) = setup_test_environment().await?;

    // Deploy a new implementation
    let new_implementation = IBlobstream::deploy(contract.provider()).await?;

    // Upgrade the contract
    contract
        .upgradeToAndCall(new_implementation.address().clone(), vec![].into())
        .send()
        .await?
        .watch()
        .await?;

    // Verify the upgrade
    let implementation_slot: U256 = U256::from_str_radix(
        "360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc",
        16,
    )
    .unwrap();
    let current_implementation = contract
        .provider()
        .get_storage_at(contract.address().clone(), implementation_slot)
        .await?;
    assert_eq!(
        &current_implementation.to_be_bytes::<32>()[12..32],
        new_implementation.address().as_slice()
    );

    // Test that the new implementation works as normal
    let tm_client = Arc::new(HttpClient::new(CELESTIA_RPC_URL)?);
    let receipt =
        prove_block_range(tm_client.clone(), BATCH_START as u64..BATCH_END as u64).await?;

    post_batch(&contract, &receipt).await?;

    let height = contract.latestHeight().call().await?;
    assert_eq!(height, BATCH_END as u64 - 1);

    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
async fn test_ownership_transfer() -> anyhow::Result<()> {
    let (anvil, contract) = setup_test_environment().await?;

    // Get the initial owner
    let initial_owner = contract.owner().call().await?;
    assert_eq!(initial_owner, anvil.addresses()[0]);

    // Transfer ownership
    let new_owner = anvil.addresses()[1];
    contract
        .transferOwnership(new_owner)
        .send()
        .await?
        .watch()
        .await?;

    // Accept ownership (need to switch to the new owner's wallet)
    let new_owner_signer: PrivateKeySigner = anvil.keys()[1].clone().into();
    let new_owner_wallet = EthereumWallet::from(new_owner_signer);
    let new_owner_provider = ProviderBuilder::new()
        .wallet(new_owner_wallet)
        .connect_http(anvil.endpoint().parse()?);
    let contract_as_new_owner = IBlobstream::new(contract.address().clone(), new_owner_provider);
    contract_as_new_owner
        .acceptOwnership()
        .send()
        .await?
        .watch()
        .await?;

    // Verify the new owner
    let final_owner = contract.owner().call().await?;
    assert_eq!(final_owner, new_owner);

    Ok(())
}
