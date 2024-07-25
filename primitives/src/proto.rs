//! Wrapper types for Tendermint types to enable protobuf (de)serialization within the zkvm.
//!
//! Types must be manually converted from the ones returned from the RPC API to be compatible,
//! but also to remove unnecessary data from being encoded into the zkvm.

use tendermint_light_client_verifier::types::{
    SignedHeader, TrustedBlockState, UntrustedBlockState, ValidatorSet,
};
use tendermint_proto::{types::LightBlock as ProtoLightBlock, Protobuf};

#[derive(Clone)]
pub struct TrustedLightBlock {
    pub signed_header: SignedHeader,
    pub next_validators: ValidatorSet,
}

impl TryFrom<ProtoLightBlock> for TrustedLightBlock {
    type Error = tendermint::Error;

    fn try_from(value: ProtoLightBlock) -> Result<Self, Self::Error> {
        Ok(Self {
            signed_header: value
                .signed_header
                .ok_or(tendermint::Error::missing_header())?
                .try_into()?,
            next_validators: value
                .validator_set
                .ok_or(tendermint::Error::missing_validator())?
                .try_into()?,
        })
    }
}

impl From<TrustedLightBlock> for ProtoLightBlock {
    fn from(value: TrustedLightBlock) -> Self {
        Self {
            signed_header: Some(value.signed_header.into()),
            validator_set: Some(value.next_validators.into()),
        }
    }
}

impl Protobuf<ProtoLightBlock> for TrustedLightBlock {}

impl TrustedLightBlock {
    /// Convert the trusted light block into type used in
    /// [Verifier::verify_update_header](tendermint_light_client_verifier::Verifier::verify_update_header)
    pub fn as_trusted_state(&self) -> TrustedBlockState<'_> {
        TrustedBlockState {
            chain_id: &self.signed_header.header.chain_id,
            header_time: self.signed_header.header.time,
            height: self.signed_header.header.height,
            next_validators: &self.next_validators,
            next_validators_hash: self.signed_header.header.next_validators_hash,
        }
    }
}

#[derive(Clone)]
pub struct UntrustedLightBlock {
    pub signed_header: SignedHeader,
    pub validators: ValidatorSet,
}

impl TryFrom<ProtoLightBlock> for UntrustedLightBlock {
    type Error = tendermint::Error;

    fn try_from(value: ProtoLightBlock) -> Result<Self, Self::Error> {
        Ok(Self {
            signed_header: value
                .signed_header
                .ok_or(tendermint::Error::missing_header())?
                .try_into()?,
            validators: value
                .validator_set
                .ok_or(tendermint::Error::missing_validator())?
                .try_into()?,
        })
    }
}

impl From<UntrustedLightBlock> for ProtoLightBlock {
    fn from(value: UntrustedLightBlock) -> Self {
        Self {
            signed_header: Some(value.signed_header.into()),
            validator_set: Some(value.validators.into()),
        }
    }
}

impl Protobuf<ProtoLightBlock> for UntrustedLightBlock {}

impl UntrustedLightBlock {
    /// Convert the untrusted light block into type used in
    /// [Verifier::verify_update_header](tendermint_light_client_verifier::Verifier::verify_update_header)
    pub fn as_untrusted_state(&self) -> UntrustedBlockState<'_> {
        UntrustedBlockState {
            signed_header: &self.signed_header,
            validators: &self.validators,
            // Note: do not need to check next validator set in zkvm, will be fetched
            // and checked in following state transition.
            next_validators: None,
        }
    }
}
