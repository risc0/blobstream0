use alloy::{
    hex::FromHex,
    network::EthereumWallet,
    node_bindings::Anvil,
    primitives::{FixedBytes, U256},
    providers::ProviderBuilder,
    signers::local::PrivateKeySigner,
};
use alloy_sol_types::{sol, SolType};
use batch_guest::BATCH_GUEST_ELF;
use host::fetch_light_block;
use light_client_guest::TM_LIGHT_CLIENT_ELF;
use reqwest::header;
use risc0_tm_core::{
    IBlobstream::{self, BinaryMerkleProof, DataRootTuple, RangeCommitment},
    LightClientCommit,
};
use risc0_zkvm::{default_prover, sha::Digestible, ExecutorEnv};
use serde::{Deserialize, Serialize};
use serde_with::{base64::Base64, serde_as, DisplayFromStr};
use tendermint::block::Height;
use tendermint_rpc::HttpClient;

sol!(
    #[sol(rpc)]
    MockVerifier,
    // TODO probably not ideal to reference build directory, fine for now.
    "../contracts/out/RiscZeroMockVerifier.sol/RiscZeroMockVerifier.json"
);

const CELESTIA_RPC_URL: &str = "https://rpc.celestia-mocha.com";

const BATCH_START: u64 = 10;
const BATCH_END: u64 = 20;
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

#[tokio::test]
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
        verifier.address().clone(),
        // Uses Celestia block hash at height
        FixedBytes::<32>::from_hex(
            "5D3BDD6B58620A0B6C5A9122863D11DA68EB18935D12A9F4E4CF1A27EB39F1AC",
        )?,
        BATCH_START,
    )
    .await?;

    let client = HttpClient::new(CELESTIA_RPC_URL)?;
    // let commit = client.latest_commit().await?;
    let query_height = Height::try_from(BATCH_START - 1)?;
    let mut previous_block = fetch_light_block(&client, query_height).await?;

    let prover = default_prover();

    let mut batch_env_builder = ExecutorEnv::builder();
    let mut batch_receipts = Vec::new();
    for height in BATCH_START..BATCH_END {
        let next_block = fetch_light_block(&client, Height::try_from(height)?).await?;

        // TODO remove the need to serialize with cbor
        // TODO a self-describing serialization protocol needs to be used with serde because the
        //      LightBlock type requires it. Seems like proto would be most stable format, rather than
        //      one used for RPC.
        let mut input_serialized = Vec::new();
        ciborium::into_writer(&(&previous_block, &next_block), &mut input_serialized)?;

        let env = ExecutorEnv::builder()
            .write_slice(&input_serialized)
            .build()?;

        let prove_info = prover.prove(env, TM_LIGHT_CLIENT_ELF)?;
        let receipt = prove_info.receipt;

        let commit: LightClientCommit = receipt.journal.decode()?;
        assert_eq!(height, commit.next_block_height);
        assert_eq!(
            next_block
                .signed_header
                .header()
                .data_hash
                .unwrap()
                .as_bytes(),
            &commit.next_data_root
        );

        batch_receipts.push(receipt.journal.bytes.clone());
        batch_env_builder.add_assumption(receipt);
        previous_block = next_block;
    }

    let env = batch_env_builder.write(&batch_receipts)?.build()?;

    let prove_info = prover.prove(env, BATCH_GUEST_ELF)?;

    let range_commitment = RangeCommitment::abi_decode(&prove_info.receipt.journal.bytes, true)?;

    // NOTE: This doesn't support bonsai, only dev mode.
    let seal: Vec<_> = [&[0u8; 4], prove_info.receipt.claim()?.digest().as_bytes()].concat();

    // Update range and await tx to be processed.
    println!("calling contract to update range");
    contract
        .updateRange(range_commitment, seal.into())
        .send()
        .await?
        .watch()
        .await?;

    let height = contract.latestHeight().call().await?;
    assert_eq!(height._0, 19);

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

    // Validate data root inclusion.
    let is_valid = contract
        .verifyAttestation(
            U256::from(1),
            DataRootTuple {
                height: U256::from(PROOF_HEIGHT),
                // TODO this is fixed from on chain, but
                dataRoot: FixedBytes::<32>::from_hex(
                    "3D96B7D238E7E0456F6AF8E7CDF0A67BD6CF9C2089ECB559C659DCAA1F880353",
                )?,
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
