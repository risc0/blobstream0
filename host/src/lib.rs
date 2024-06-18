use tendermint::{block::Height, node::Id, validator::Set};
use tendermint_light_client_verifier::types::LightBlock;
use tendermint_rpc::{Client, HttpClient, Paging};

pub async fn fetch_light_block(
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
