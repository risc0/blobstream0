use alloy_primitives::U256;
use alloy_sol_types::SolValue;
use light_client_guest::TM_LIGHT_CLIENT_ID;
use risc0_tm_core::{DataRootTuple, LightClientCommit, MerkleTree};
use risc0_zkvm::{guest::env, serde::from_slice};

type JournalBytes = Vec<u8>;

fn main() {
    // Input the vector of proofs to batch.
    let input: Vec<JournalBytes> = env::read();

    let mut verified_blocks: Vec<DataRootTuple> = Vec::with_capacity(input.len());
    for journal in input {
        env::verify(TM_LIGHT_CLIENT_ID, &journal).unwrap();
        let commit: LightClientCommit = from_slice(&journal).unwrap();
        if let Some(prev_hash) = verified_blocks.last() {
            // Assert that the blocks hash link to each other.
            assert_eq!(&commit.first_data_root, &prev_hash.dataRoot);
        }
        verified_blocks.push(DataRootTuple {
            height: U256::from(commit.next_block_height),
            dataRoot: commit.next_data_root.into(),
        });
    }

    // TODO this is a bit inefficient, since we don't need to keep intermediate nodes in heap.
    //      ideally this just generates hash and drops any intermediate value. (minor opt)
    let root = MerkleTree::from_leaves(&verified_blocks).root();

    // TODO switch this return type to be connected to eth contract and typesafe.
    env::commit_slice(root.abi_encode().as_slice());
}
