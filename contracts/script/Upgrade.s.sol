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

pragma solidity ^0.8.24;

import {Script} from "forge-std/Script.sol";
import {console2} from "forge-std/console2.sol";
import "forge-std/Test.sol";
import {IRiscZeroVerifier} from "risc0/IRiscZeroVerifier.sol";
import {RiscZeroGroth16Verifier} from "risc0/groth16/RiscZeroGroth16Verifier.sol";
import {ControlID} from "risc0/groth16/ControlID.sol";

import {Blobstream0} from "../src/Blobstream0.sol";
import {ERC1967Proxy} from "openzeppelin/contracts/proxy/ERC1967/ERC1967Proxy.sol";

/// See the Foundry documentation for more information about Solidity scripts,
/// including information about wallet options.
///
/// https://book.getfoundry.sh/tutorials/solidity-scripting
/// https://book.getfoundry.sh/reference/forge/forge-script
contract Blobstream0Upgrade is Script {
    function run() external {
        uint256 deployerPrivateKey = vm.envUint("PRIVATE_KEY");
        address addr = vm.envAddress("PROXY_ADDRESS");
        address payable proxy_addr = payable(address(uint160(addr)));
        Blobstream0 proxy = Blobstream0(proxy_addr);
        vm.startBroadcast(deployerPrivateKey);

        Blobstream0 impl = new Blobstream0();
        console2.log("Upgrading to new implementation address:", address(impl));

        proxy.upgradeToAndCall(address(impl), bytes(""));

        vm.stopBroadcast();
    }
}
