use alloy_primitives::U256;
use alloy_sol_types::SolValue;
use light_client_guest::TM_LIGHT_CLIENT_ID;
use risc0_tm_core::{DataRootTuple, LightClientCommit, MerkleTree, RangeCommitment};
use risc0_zkvm::{guest::env, serde::from_slice};

// TODO by default this will serialize poorly, optimize
type JournalBytes = Vec<u8>;

fn main() {
    // Input the vector of proofs to batch.
    let input: Vec<JournalBytes> = env::read();

    let mut trusted_header_hash = None;
    let mut merkle_tree = MerkleTree::default();
    let mut last_verified: Option<LightClientCommit> = None;
    for journal in input {
        env::verify(TM_LIGHT_CLIENT_ID, &journal).unwrap();
        let commit: LightClientCommit = from_slice(&journal).unwrap();
        let height = U256::from(commit.next_block_height);
        if let Some(prev_verified) = last_verified.as_ref() {
            // Assert that the previous block commitment equals the next.
            assert_eq!(&commit.trusted_block_hash, &prev_verified.next_block_hash);

            // TODO look into how skipped blocks are handled, will it be a missing height?
            // TODO perhaps commit the block header's parent hash, to compare to make sure no gaps?
        } else {
            trusted_header_hash = Some(commit.trusted_block_hash);
        }

        // TODO this is a bit inefficient, since we don't need to keep intermediate nodes in heap.
        //      ideally this just generates hash and drops any intermediate value. (minor opt)
        merkle_tree.push(&DataRootTuple {
            height,
            dataRoot: commit.next_data_root.into(),
        });

        // Set the most recently validated block, to validate the next against.
        last_verified = Some(commit);
    }

    let latest_block = last_verified.unwrap();
    let commit = RangeCommitment {
        trustedHeaderHash: trusted_header_hash
            .expect("must be at least one verified block")
            .into(),
        newHeight: latest_block.next_block_height,
        newHeaderHash: latest_block.next_block_hash.into(),
        merkleRoot: merkle_tree.root().into(),
    };
    env::commit_slice(commit.abi_encode().as_slice());
}
