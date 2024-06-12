use core::time::Duration;
use risc0_zkvm::guest::env;
use tendermint_light_client_verifier::{
    options::Options, types::LightBlock, ProdVerifier, Verdict, Verifier,
};

fn main() {
    let (light_block_previous, light_block_next): (LightBlock, LightBlock) =
        ciborium::from_reader(env::stdin()).unwrap();

    let vp = ProdVerifier::default();
    let opt = Options {
        trust_threshold: Default::default(),
        // TODO check these options (value pulled from TM repo)
        trusting_period: Duration::from_secs(60),
        clock_drift: Default::default(),
    };
    // TODO this verify time picked pretty arbitrarily, need to be after header time and within
    // trusting window.
    let verify_time = light_block_next.time() + Duration::from_secs(20);
    let verdict = vp.verify_update_header(
        // TODO should assert that next_validators is Some
        light_block_next.as_untrusted_state(),
        light_block_previous.as_trusted_state(),
        &opt,
        verify_time.unwrap(),
    );

    // TODO check trusted.next_validators.hash() == trusted.next_validators_hash (might be done implicitly)

    assert!(
        matches!(verdict, Verdict::Success),
        "validation failed, {:?}",
        verdict
    );

    env::commit_slice(light_block_previous.signed_header.header.hash().as_bytes());
    env::commit_slice(light_block_next.signed_header.header.hash().as_bytes());
}
