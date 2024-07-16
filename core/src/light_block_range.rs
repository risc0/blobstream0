use tendermint_light_client_verifier::types::{Header, LightBlock};
use serde_tuple::{Serialize_tuple, Deserialize_tuple};

pub(crate) struct LightBlockRangeIterator<'a> {
    pub trusted_block: &'a LightBlock,
    pub blocks: &'a [LightBlock],
}

/// Inputs for light client block proving for Blobstream. Serialized as tuple for more compact form.
#[derive(Serialize_tuple, Deserialize_tuple)]
pub(crate) struct LightBlockProveData {
    pub trusted_block: LightBlock,
    pub interval_headers: Vec<Header>,
    pub target_block: LightBlock,
}

impl LightBlockProveData {
	/// Height of the block to prove to.
	pub fn target_height(&self) -> u64 {
		self.target_block.signed_header.header.height.value()
	}

	/// Trusted height for the starting point of the proof.
	pub fn trusted_height(&self) -> u64 {
		self.trusted_block.signed_header.header.height.value()
	}
}

impl Iterator for LightBlockRangeIterator<'_> {
    type Item = LightBlockProveData;

    fn next(&mut self) -> Option<Self::Item> {
		// TODO
		todo!();
    }
}
