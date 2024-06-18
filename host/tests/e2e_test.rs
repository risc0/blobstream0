use alloy::{
    network::EthereumWallet, node_bindings::Anvil, providers::ProviderBuilder,
    signers::local::PrivateKeySigner,
};
use alloy_sol_types::{sol, SolCall};
use batch_guest::BATCH_GUEST_ELF;
use host::fetch_light_block;
use light_client_guest::TM_LIGHT_CLIENT_ELF;
use reqwest::header;
use risc0_tm_core::IBlobstream;
use risc0_zkvm::{default_prover, ExecutorEnv};
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
const BATCH_PROOF: u64 = 15;

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
    let contract = IBlobstream::deploy(&provider, verifier.address().clone()).await?;

    // Somewhat hacky to do this manually, seems no Rust tooling for this.
    let http_client = reqwest::Client::new();
    let proof_response = http_client
        .get(format!(
            "{}/data_root_inclusion_proof?height={}&start={}&end={}",
            CELESTIA_RPC_URL, BATCH_PROOF, BATCH_START, BATCH_END
        ))
        .header(header::CONTENT_TYPE, "application/json")
        .send()
        .await?;

    let response = proof_response.json::<serde_json::Value>().await?;
    let response: DataRootInclusionResponse =
        serde_json::from_value(response["result"]["proof"].clone())?;

    let client = HttpClient::new(CELESTIA_RPC_URL)?;
    // let commit = client.latest_commit().await?;
    let query_height = Height::try_from(BATCH_START - 1)?;
    let mut previous_block = fetch_light_block(&client, query_height).await?;

    let prover = default_prover();

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

        batch_receipts.push(receipt);
        previous_block = next_block;
    }
    let mut batch_env_builder = ExecutorEnv::builder();
    let batch_journals: Vec<Vec<u8>> = batch_receipts
        .iter()
        .map(|r| r.journal.bytes.clone())
        .collect();
    for receipt in batch_receipts {
        batch_env_builder.add_assumption(receipt);
    }

    let env = batch_env_builder.write(&batch_journals)?.build()?;

    let prove_info = prover.prove(env, BATCH_GUEST_ELF)?;

    // Validate proof on-chain.
    // TODO

    // Validate data root inclusion.
    // TODO

    Ok(())
}
