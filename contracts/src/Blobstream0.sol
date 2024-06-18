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
import "./RangeCommitment.sol";
import "blobstream/DataRootTuple.sol";
import "blobstream/lib/tree/binary/BinaryMerkleTree.sol";

/// @title A starter application using RISC Zero.
contract Blobstream0 is IDAOracle {
    /// @notice RISC Zero verifier contract address.
    IRiscZeroVerifier public immutable verifier;

    /// @notice Image ID of the only zkVM binary to accept verification from.
    ///         The image ID is similar to the address of a smart contract.
    ///         It uniquely represents the logic of that guest program,
    ///         ensuring that only proofs generated from a pre-defined guest program
    ///         (in this case, checking if a number is even) are considered valid.
    bytes32 public constant imageId = ImageID.BATCH_GUEST_ID;

    // TODO this isn't used, update nonce on batch validation.
    uint256 public proofNonce;

    /// @notice This is a mapping of heights to merkle roots at those heights.
    mapping(uint256 => bytes32) merkleRoots;

    /// @notice Initialize the contract, binding it to a specified RISC Zero verifier.
    constructor(IRiscZeroVerifier _verifier) {
        verifier = _verifier;
    }

    /// @notice Validate a proof of a new header range, update state.
    function updateRange(
        RangeCommitment memory _commit,
        bytes calldata _seal
    ) external {
        bytes memory journal = abi.encode(_commit);
        verifier.verify(_seal, imageId, sha256(journal));
    }

    function verifyAttestation(
        uint256 _proofNonce,
        DataRootTuple memory _tuple,
        BinaryMerkleProof memory _proof
    ) external view returns (bool) {
        if (_proofNonce == 0 || _proofNonce >= proofNonce) {
            return false;
        }

        bytes32 root = merkleRoots[_proofNonce];

        (bool isProofValid, ) = BinaryMerkleTree.verify(
            root,
            _proof,
            abi.encode(_tuple)
        );

        return isProofValid;
    }
}
