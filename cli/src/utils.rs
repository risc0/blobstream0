use alloy::{
    network::{Network, TransactionBuilder},
    providers::{
        fillers::{FillerControlFlow, TxFiller},
        Provider, SendableTx,
    },
    transports::{Transport, TransportResult},
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FireblocksFiller;

impl<N: Network> TxFiller<N> for FireblocksFiller {
    type Fillable = ();

    fn status(&self, _tx: &<N as Network>::TransactionRequest) -> FillerControlFlow {
        FillerControlFlow::Finished
    }

    fn fill_sync(&self, tx: &mut SendableTx<N>) {
        if let Some(builder) = tx.as_mut_builder() {
            // 1 Ether max tx fee
            if let Some(fee) = builder.max_fee_per_gas() {
                // builder.set_max_fee_per_gas(core::cmp::min(0x0de0b6b3a7640000, fee));
                builder.set_max_fee_per_gas(1000000000);
            }

            // Set gas limit manually
            builder.set_gas_limit(2_000_000);

            // builder.set
        }
    }

    async fn prepare<P, T>(
        &self,
        _provider: &P,
        _tx: &<N as Network>::TransactionRequest,
    ) -> TransportResult<Self::Fillable>
    where
        P: Provider<T, N>,
        T: Transport + Clone,
    {
        Ok(())
    }

    async fn fill(
        &self,
        _fillable: Self::Fillable,
        tx: SendableTx<N>,
    ) -> TransportResult<SendableTx<N>> {
        Ok(tx)
    }
}
