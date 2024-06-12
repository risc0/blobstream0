use core::time::Duration;
use risc0_tm_core::LightClientCommit;
use risc0_zkvm::guest::env;
use tendermint::Hash;
use tendermint_light_client_verifier::{
    options::Options,
    types::{LightBlock, TrustThreshold},
    ProdVerifier, Verdict, Verifier,
};

fn main() {
    // TODO this probably wants to be protobuf
    let (light_block_previous, light_block_next): (LightBlock, LightBlock) =
        ciborium::from_reader(env::stdin()).unwrap();

    let vp = ProdVerifier::default();
    let opt = Options {
        // Trust threshold overriden to match security used by IBC default
        // See context https://github.com/informalsystems/hermes/issues/2876
        trust_threshold: TrustThreshold::TWO_THIRDS,
        trusting_period: Duration::from_secs(64000),
        clock_drift: Default::default(),
    };

    // Check the next_validators hash, as verify_update_header leaves it for caller to check.
    let trusted_state = light_block_previous.as_trusted_state();
    assert_eq!(
        trusted_state.next_validators.hash(),
        trusted_state.next_validators_hash
    );

    let untrusted_state = light_block_next.as_untrusted_state();
    // Assert that next validators is provided, such that verify will check it.
    // Note: this is a bit redundant, given converting from LightBlock will always be Some,
    // but this is to be sure the check is always done, even if refactored.
    assert!(untrusted_state.next_validators.is_some());

    // This verify time picked pretty arbitrarily, need to be after header time and within
    // trusting window.
    let verify_time = light_block_next.time() + Duration::from_secs(1);
    let verdict =
        vp.verify_update_header(untrusted_state, trusted_state, &opt, verify_time.unwrap());

    assert!(
        matches!(verdict, Verdict::Success),
        "validation failed, {:?}",
        verdict
    );

    // TODO also mixing serialization with using default, resolve with above
    env::commit(&LightClientCommit {
        first_block_hash: expect_sha256_hash(&light_block_previous),
        next_block_hash: expect_sha256_hash(&light_block_next),
    });
}

fn expect_sha256_hash(block: &LightBlock) -> [u8; 32] {
    let Hash::Sha256(first_block_hash) = block.signed_header.header().hash() else {
        unreachable!("Header hash should always be a non empty sha256");
    };
    first_block_hash
}
