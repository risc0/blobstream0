use light_client_guest::TM_LIGHT_CLIENT_ID;
use risc0_tm_core::{LightClientCommit, MerkleHash, MerkleTree};
use risc0_zkvm::{guest::env, serde::from_slice};
use alloy_sol_types::SolValue;

type JournalBytes = Vec<u8>;

fn main() {
    // Input the vector of proofs to batch.
    let input: Vec<JournalBytes> = env::read();

    let mut verified_blocks: Vec<MerkleHash> = Vec::with_capacity(input.len());
    for journal in input {
        env::verify(TM_LIGHT_CLIENT_ID, &journal).unwrap();
        let commit: LightClientCommit = from_slice(&journal).unwrap();
        if let Some(prev_hash) = verified_blocks.last() {
            // Assert that the blocks hash link to each other.
            assert_eq!(&commit.first_block_hash, prev_hash);
        }
        verified_blocks.push(commit.next_block_hash);
    }


    // TODO this is a bit inefficient, since we don't need to keep intermediate nodes in heap.
    //      ideally this just generates hash and drops any intermediate value. (minor opt)
    let root = MerkleTree::from_leaves(verified_blocks).root();

    // TODO switch this return type to be connected to eth contract and typesafe.
    env::commit_slice(root.abi_encode().as_slice());
}
