use risc0_zkvm::{compute_image_id, default_prover, ExecutorEnv};
use tendermint::{block::Height, node::Id, validator::Set};
use tendermint_light_client_verifier::types::LightBlock;
use tendermint_rpc::{Client, HttpClient, Paging};

const ELF: &[u8] = include_bytes!("../../target/riscv32im-risc0-zkvm-elf/release/guest");
const CELESTIA_RPC_URL: &str = "https://rpc.celestia-mocha.com";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing. In order to view logs, run `RUST_LOG=info cargo run`
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::filter::EnvFilter::from_default_env())
        .init();

    let client = HttpClient::new(CELESTIA_RPC_URL)?;
    let commit = client.latest_commit().await?;

    // Fetch latest commit just for block number.
    let block_num = commit.signed_header.header.height;
    println!("fetching blocks at height: {}", block_num);

    // Retrieve past blocks
    let next_block = fetch_light_block(&client, block_num).await?;
    // We could request further back, as long as no more than 1/3 validators rotated between,
    // but for this use case, we will be validating every block.
    let previous_block =
        fetch_light_block(&client, Height::try_from(block_num.value() - 2)?).await?;

    // TODO remove the need to serialize with cbor
    // TODO a self-describing serialization protocol needs to be used with serde because the
    //      LightBlock type requires it. Seems like proto would be most stable format, rather than
    //      one used for RPC.
    let mut input_serialized = Vec::new();
    ciborium::into_writer(&(&previous_block, &next_block), &mut input_serialized)?;

    let guest_id = compute_image_id(ELF)?;
    let env = ExecutorEnv::builder()
        .write_slice(&input_serialized)
        .build()?;

    let prover = default_prover();

    let prove_info = prover.prove(env, ELF)?;
    let receipt = prove_info.receipt;

    receipt.verify(guest_id)?;

    Ok(())
}

async fn fetch_light_block(
    client: &HttpClient,
    block_height: Height,
) -> anyhow::Result<LightBlock> {
    let commit_response = client.commit(block_height).await?;
    let signed_header = commit_response.signed_header;
    let height = signed_header.header.height;

    // Note: This currently needs to use Paging::All or the hash mismatches.
    let validator_response = client.validators(height, Paging::All).await?;

    let validators = Set::new(validator_response.validators, None);

    let next_validator_response = client.validators(height.increment(), Paging::All).await?;
    let next_validators = Set::new(next_validator_response.validators, None);

    Ok(LightBlock::new(
        signed_header,
        validators,
        next_validators,
        // TODO do we care about this ID?
        Id::new([0; 20]),
    ))
}
// }
