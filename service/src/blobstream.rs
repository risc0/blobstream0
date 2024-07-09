use risc0_tm_core::IBlobstream::IBlobstreamInstance;
use tendermint_rpc::HttpClient;

pub(crate) struct BlobstreamService<T, P, N> {
    contract: IBlobstreamInstance<T, P, N>,
    tm_client: HttpClient,
}

impl<T, P, N> BlobstreamService<T, P, N> {
    pub fn new(contract: IBlobstreamInstance<T, P, N>, tm_client: HttpClient) -> Self {
        Self {
            contract,
            tm_client,
        }
    }
}
