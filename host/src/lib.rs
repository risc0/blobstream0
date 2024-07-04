use batch_guest::BATCH_GUEST_ELF;
use light_client_guest::TM_LIGHT_CLIENT_ELF;
use risc0_tm_core::LightClientCommit;
use risc0_zkvm::{default_prover, ExecutorEnv, Prover, Receipt};
use std::ops::Range;
use tendermint::{block::Height, node::Id, validator::Set};
use tendermint_light_client_verifier::types::LightBlock;
use tendermint_rpc::{Client, HttpClient, Paging};

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

/// Contains the receipt and light client block that was proven.
/// Can be constructed with [`prove_block`].
pub struct LightBlockProof {
    receipt: Receipt,
    light_block: LightBlock,
}

/// Prove a single block with the trusted light client block and the height to fetch and prove.
pub async fn prove_block(
    prover: &dyn Prover,
    client: &HttpClient,
    previous_block: &LightBlock,
    height: u64,
) -> anyhow::Result<LightBlockProof> {
    let next_block = fetch_light_block(&client, Height::try_from(height)?).await?;

    // TODO remove the need to serialize with cbor
    // TODO a self-describing serialization protocol needs to be used with serde because the
    //      LightBlock type requires it. Seems like proto would be most stable format, rather than
    //      one used for RPC.
    let mut input_serialized = Vec::new();
    ciborium::into_writer(&(previous_block, &next_block), &mut input_serialized)?;

    let env = ExecutorEnv::builder()
        .write_slice(&input_serialized)
        .build()?;

    // Note: must block in place to not have issues with Bonsai blocking client when selected
    let prove_info = tokio::task::block_in_place(|| prover.prove(env, TM_LIGHT_CLIENT_ELF))?;
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

    Ok(LightBlockProof {
        receipt,
        light_block: next_block,
    })
}

/// Fetches and proves a range of light client blocks.
pub async fn prove_block_range(client: &HttpClient, range: Range<u64>) -> anyhow::Result<Receipt> {
    let prover = default_prover();

    let query_height = Height::try_from(range.start - 1)?;
    let mut previous_block = fetch_light_block(&client, query_height).await?;

    let mut batch_env_builder = ExecutorEnv::builder();
    let mut batch_receipts = Vec::new();
    // TODO(opt): Retrieving light blocks and proving can be parallelized
    for height in range {
        // TODO this will likely have to check chain height and wait for new block to be published
        //      or have a separate function do this.
        let LightBlockProof {
            receipt,
            light_block,
        } = prove_block(prover.as_ref(), client, &previous_block, height).await?;

        batch_receipts.push(receipt.journal.bytes.clone());
        batch_env_builder.add_assumption(receipt);
        previous_block = light_block;
    }

    let env = batch_env_builder.write(&batch_receipts)?.build()?;

    // Note: must block in place to not have issues with Bonsai blocking client when selected
    let prove_info = tokio::task::block_in_place(|| prover.prove(env, BATCH_GUEST_ELF))?;

    Ok(prove_info.receipt)
}
