// Copyright 2024 RISC Zero, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// SPDX-License-Identifier: Apache-2.0

pragma solidity ^0.8.20;

import {IRiscZeroVerifier} from "risc0/IRiscZeroVerifier.sol";
import {ImageID} from "./ImageID.sol"; // auto-generated contract after running `cargo build`.
import {IDAOracle} from "blobstream/IDAOracle.sol";
import {Ownable, Ownable2Step} from "openzeppelin/contracts/access/Ownable2Step.sol";
import "./RangeCommitment.sol";
import "blobstream/DataRootTuple.sol";
import "blobstream/lib/tree/binary/BinaryMerkleTree.sol";

/// @title A starter application using RISC Zero.
contract Blobstream0 is IDAOracle, Ownable2Step {
    /// @notice Data commitment stored for the block range [startBlock, endBlock) with proof nonce.
    /// NOTE: This event matches existing Blobstream contracts, for ease of integration.
    /// @param proofNonce The nonce of the proof.
    /// @param startBlock The start block of the block range.
    /// @param endBlock The end block of the block range.
    /// @param dataCommitment The data commitment for the block range.
    event DataCommitmentStored(
        uint256 proofNonce, uint64 indexed startBlock, uint64 indexed endBlock, bytes32 indexed dataCommitment
    );

    /// @notice Emits event with the new head update.
    /// NOTE: Matches existing Blobstream contract, for ease of integration.
    event HeadUpdate(uint64 blockNumber, bytes32 headerHash);

    /// @notice Target height for next batch was below the current height.
    error InvalidTargetHeight();

    /// @notice Trusted block hash does not equal the commitment from the new batch.
    error InvalidTrustedHeaderHash();

    /// @notice RISC Zero verifier contract address.
    IRiscZeroVerifier public verifier;

    /// @notice Image ID of the only zkVM binary to accept verification from.
    ///         The image ID is similar to the address of a smart contract.
    ///         It uniquely represents the logic of that guest program,
    ///         ensuring that only proofs generated from a pre-defined guest program.
    bytes32 public imageId = ImageID.LIGHT_CLIENT_GUEST_ID;

    /// @notice nonce for mapping block ranges to block merkle roots. This value is used as the key
    ///         to insert new roots in `merkleRoots`.
    uint256 public proofNonce;

    /// @notice The latest height validated.
    /// @dev this value is 64 bits as is the max for heights in Tendermint.
    uint64 public latestHeight;

    /// @notice The latest block hash validated.
    /// @dev always update this in tandem with `latestHeight`
    // TODO product test if useful to store historical hashes since they are already available?
    bytes32 public latestBlockHash;

    /// @notice This is a mapping of proof nonces to merkle roots at those heights.
    mapping(uint256 => bytes32) merkleRoots;

    /// @notice Initialize the contract, binding it to a specified RISC Zero verifier.
    constructor(address _admin, IRiscZeroVerifier _verifier, bytes32 _trustedHash, uint64 _trustedHeight)
        Ownable(_admin)
    {
        verifier = _verifier;
        latestBlockHash = _trustedHash;
        latestHeight = _trustedHeight;

        // Proof nonce initialized as 1 to maintain compatibility with existing implementations and
        // avoid default value confusion.
        proofNonce = 1;
    }

    /// @notice Only the admin can update. Updates the trusted height and block hash to sync from.
    function adminSetTrustedState(bytes32 _trustedHash, uint64 _trustedHeight) external onlyOwner {
        latestBlockHash = _trustedHash;
        latestHeight = _trustedHeight;
    }

    /// @notice Only the admin can update. Updates the image ID to verify proofs against.
    function adminSetImageId(bytes32 _imageId) external onlyOwner {
        imageId = _imageId;
    }

    /// @notice Only the admin can update. Updates the verifier contract address.
    function adminSetVerifier(IRiscZeroVerifier _verifier) external onlyOwner {
        verifier = _verifier;
    }

    /// @notice Validate a proof of a new header range, update state.
    function updateRange(bytes calldata _commitBytes, bytes calldata _seal) external {
        RangeCommitment memory commit = abi.decode(_commitBytes, (RangeCommitment));

        if (commit.newHeight <= latestHeight) {
            revert InvalidTargetHeight();
        }
        if (commit.trustedHeaderHash != latestBlockHash) {
            revert InvalidTrustedHeaderHash();
        }
        verifier.verify(_seal, imageId, sha256(_commitBytes));

        emit DataCommitmentStored(proofNonce, latestHeight, commit.newHeight, commit.merkleRoot);

        // Update latest block in state
        latestHeight = commit.newHeight;
        latestBlockHash = commit.newHeaderHash;
        emit HeadUpdate(latestHeight, latestBlockHash);

        // Set merkle root to monotomically increasing nonce. This is kept as is for compatibility
        // with alternative versions.
        merkleRoots[proofNonce] = commit.merkleRoot;
        proofNonce++;
    }

    /// @notice Verify a Data Availability attestation. Method of IDAOracle from Blobstream
    /// contract.
    /// @param _proofNonce Nonce of the tuple root to prove against.
    /// @param _tuple Data root tuple to prove inclusion of.
    /// @param _proof Binary Merkle tree proof that `tuple` is in the root at `_tupleRootNonce`.
    /// @return `true` is proof is valid, `false` otherwise.
    function verifyAttestation(uint256 _proofNonce, DataRootTuple memory _tuple, BinaryMerkleProof memory _proof)
        external
        view
        returns (bool)
    {
        if (_proofNonce == 0 || _proofNonce >= proofNonce) {
            return false;
        }

        bytes32 root = merkleRoots[_proofNonce];

        (bool isProofValid,) = BinaryMerkleTree.verify(root, _proof, abi.encode(_tuple));

        return isProofValid;
    }
}
