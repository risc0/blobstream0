use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct LightClientCommit {
    pub first_block_hash: [u8; 32],
    pub next_block_hash: [u8; 32],
}
