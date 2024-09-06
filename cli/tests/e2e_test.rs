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
    hex::FromHex,
    network::EthereumWallet,
    node_bindings::Anvil,
    primitives::{Bytes, FixedBytes, U256},
    providers::ProviderBuilder,
    signers::local::PrivateKeySigner,
};
use alloy_sol_types::sol;
use blobstream0_core::{post_batch, prove_block_range};
use blobstream0_primitives::IBlobstream::{self, BinaryMerkleProof, DataRootTuple};
use reqwest::header;
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, hex::Hex, serde_as, DisplayFromStr};
use std::sync::Arc;
use tendermint_rpc::HttpClient;
use ShareLoader::{
    AttestationProof, Namespace, NamespaceMerkleMultiproof, NamespaceNode, SharesProof,
};

sol!(
    #[sol(rpc)]
    MockVerifier,
    "../contracts/out/RiscZeroMockVerifier.sol/RiscZeroMockVerifier.json"
);

sol!(
    #[derive(Debug)]
    #[sol(rpc)]
    ShareLoader,
    "../contracts/out/ShareLoader.t.sol/ShareLoader.json"
);

const CELESTIA_RPC_URL: &str = "https://celestia-testnet.brightlystake.com";

const BATCH_START: u64 = 10;
const BATCH_END: u64 = 42;
const PROOF_HEIGHT: u64 = 15;

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

    // Deploy the contract.
    let contract = IBlobstream::deploy(
        &provider,
        anvil.addresses()[0],
        verifier.address().clone(),
        // Uses Celestia block hash at height below proving range on Mocha
        FixedBytes::<32>::from_hex(
            "5C5451567973D8658A607D58F035BA9078291E33D880A0E6E67145C717E6B11B",
        )?,
        BATCH_START - 1,
    )
    .await?;

    let client = Arc::new(HttpClient::new(CELESTIA_RPC_URL)?);

    let receipt = prove_block_range(client, BATCH_START..BATCH_END).await?;

    post_batch(&contract, &receipt).await?;

    let height = contract.latestHeight().call().await?;
    assert_eq!(height._0, BATCH_END - 1);

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

    let inclusion_proof: Vec<FixedBytes<32>> = response
        .aunts
        .iter()
        .map(|node| node.as_slice().try_into().unwrap())
        .collect();

    let tuple_root_nonce = U256::from(1);
    #[allow(non_snake_case)]
    let dataRoot = FixedBytes::<32>::from_hex(
        "3D96B7D238E7E0456F6AF8E7CDF0A67BD6CF9C2089ECB559C659DCAA1F880353",
    )?;
    let tuple = DataRootTuple {
        height: U256::from(PROOF_HEIGHT),
        // TODO this is fixed from on chain, but could be pulled from node to be dynamic
        dataRoot,
    };
    let binary_merkle_proof = BinaryMerkleProof {
        sideNodes: inclusion_proof.clone(),
        key: U256::from(response.index),
        numLeaves: U256::from(response.total),
    };

    // Validate data root inclusion.
    let is_valid = contract
        .verifyAttestation(tuple_root_nonce, tuple, binary_merkle_proof)
        .call()
        .await?;
    assert!(is_valid._0);

    let attestation_proof = AttestationProof {
        tupleRootNonce: U256::from(1),
        tuple: ShareLoader::DataRootTuple {
            height: U256::from(PROOF_HEIGHT),
            // TODO this is fixed from on chain, but could be pulled from node to be dynamic
            dataRoot,
        },
        proof: ShareLoader::BinaryMerkleProof {
            sideNodes: inclusion_proof.clone(),
            key: U256::from(response.index),
            numLeaves: U256::from(response.total),
        },
    };

    #[serde_as]
    #[derive(Debug, Deserialize)]
    struct SharesProofJson {
        #[serde_as(as = "Vec<Base64>")]
        data: Vec<Bytes>,
        share_proofs: Vec<NamespaceMerkleMultiproofJson>,
        #[serde_as(as = "Base64")]
        namespace_id: [u8; 28],
        row_proof: RowProofJson,
        namespace_version: u8,
    }

    impl SharesProofJson {
        fn into_sol(self, attestation_proof: AttestationProof) -> SharesProof {
            let share_proofs = self
                .share_proofs
                .into_iter()
                .map(|p| NamespaceMerkleMultiproof {
                    beginKey: U256::from(p.start),
                    endKey: U256::from(p.end),
                    sideNodes: p.nodes.iter().map(|n| to_namespace_node(n)).collect(),
                })
                .collect();
            let row_roots = self
                .row_proof
                .row_roots
                .into_iter()
                .map(|n| to_namespace_node(&n))
                .collect();

            let row_proofs = self
                .row_proof
                .proofs
                .into_iter()
                .map(|p| ShareLoader::BinaryMerkleProof {
                    sideNodes: p
                        .aunts
                        .into_iter()
                        .map(|b| FixedBytes::<32>::from(b))
                        .collect(),
                    key: U256::from(p.index),
                    numLeaves: U256::from(p.total),
                })
                .collect();

            SharesProof {
                data: self.data,
                shareProofs: share_proofs,
                namespace: namespace(self.namespace_id, self.namespace_version),
                rowRoots: row_roots,
                rowProofs: row_proofs,
                attestationProof: attestation_proof,
            }
        }
    }

    fn min_namespace(inner_node: &[u8]) -> Namespace {
        let version = FixedBytes::from([inner_node[0]]);
        let id = inner_node[1..29].try_into().unwrap();
        Namespace { version, id }
    }

    fn max_namespace(inner_node: &[u8]) -> Namespace {
        let version = FixedBytes::from([inner_node[29]]);
        let id = inner_node[30..58].try_into().unwrap();
        Namespace { version, id }
    }

    fn to_namespace_node(node: &[u8]) -> NamespaceNode {
        let min_ns = min_namespace(node);
        let max_ns = max_namespace(node);
        let digest: FixedBytes<32> = node[58..].try_into().unwrap();
        NamespaceNode {
            min: min_ns,
            max: max_ns,
            digest,
        }
    }

    fn namespace(namespace_id: [u8; 28], version: u8) -> Namespace {
        let version = FixedBytes::from([version]);
        let id = FixedBytes::from(namespace_id);
        Namespace { version, id }
    }

    #[serde_as]
    #[derive(Debug, Deserialize)]
    struct NamespaceMerkleMultiproofJson {
        #[serde(default)]
        start: u64,
        end: u64,
        #[serde_as(as = "Vec<Base64>")]
        nodes: Vec<Vec<u8>>,
    }

    #[serde_as]
    #[derive(Debug, Deserialize)]
    struct RowProofJson {
        #[serde_as(as = "Vec<Hex>")]
        // NOTE: This is abi encoded hex??
        row_roots: Vec<Vec<u8>>,
        proofs: Vec<BinaryMerkleProofJson>,
        // TODO this is unused.
        start_row: u64,
        // TODO this is unused.
        end_row: u64,
    }

    #[serde_as]
    #[derive(Debug, Deserialize)]
    struct BinaryMerkleProofJson {
        #[serde_as(as = "DisplayFromStr")]
        total: u64,
        #[serde_as(as = "DisplayFromStr")]
        index: u64,
        #[serde_as(as = "Base64")]
        // TODO this is unused.
        leaf_hash: Vec<u8>,
        #[serde_as(as = "Vec<Base64>")]
        aunts: Vec<[u8; 32]>,
    }

    let proof_response = http_client
        .get(format!(
            "{}/prove_shares_v2?height={}&startShare=0&endShare=1",
            CELESTIA_RPC_URL, PROOF_HEIGHT
        ))
        .header(header::CONTENT_TYPE, "application/json")
        .send()
        .await?;

    let response = proof_response.json::<serde_json::Value>().await?;
    let response: SharesProofJson =
        serde_json::from_value(response["result"]["share_proof"].clone())?;

    let s = response.into_sol(attestation_proof);

    let share_loader = ShareLoader::deploy(&provider, contract.address().clone()).await?;

    let a = share_loader.verifyShares(s).call().await?;
    assert_eq!(a._0, true, "invalid share proof");
    assert_eq!(a._1, 0, "invalid error code");

    Ok(())
}
