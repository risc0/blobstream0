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

import {RiscZeroCheats} from "risc0/test/RiscZeroCheats.sol";
import {console2} from "forge-std/console2.sol";
import {Test} from "forge-std/Test.sol";
import {IRiscZeroVerifier} from "risc0/IRiscZeroVerifier.sol";
import {Blobstream0} from "../src/Blobstream0.sol";
import {Elf} from "./Elf.sol"; // auto-generated contract after running `cargo build`.

// TODO I will probably just remove this altogether.
contract Blobstream0Test is RiscZeroCheats, Test {
    Blobstream0 public blobstream;

    function setUp() public {
        IRiscZeroVerifier verifier = deployRiscZeroVerifier();
        blobstream = new Blobstream0(verifier, 0x5D3BDD6B58620A0B6C5A9122863D11DA68EB18935D12A9F4E4CF1A27EB39F1AC, 10);
    }

    function test_SetEven() public {
        // uint256 number = 12345678;
        // (bytes memory journal, bytes memory seal) = prove(
        //     Elf.BATCH_GUEST_PATH,
        //     abi.encode(number)
        // );
        // blobstream.set(abi.decode(journal, (uint256)), seal);
        // assertEq(blobstream.get(), number);
    }
}
