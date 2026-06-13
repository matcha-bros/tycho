// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import { Script } from "forge-std/Script.sol";
import { AquaSwapVMSwapAdapter } from "../src/AquaSwapVMSwapAdapter.sol";
import { MockAquaSwapVMRouter } from "../src/MockAquaSwapVMRouter.sol";

contract BuildRuntime is Script {
    function run() external {
        AquaSwapVMSwapAdapter adapter = new AquaSwapVMSwapAdapter();
        vm.writeFileBinary("out/AquaSwapVMSwapAdapter.evm.runtime", address(adapter).code);

        MockAquaSwapVMRouter router = new MockAquaSwapVMRouter();
        vm.writeFileBinary("out/MockAquaSwapVMRouter.evm.runtime", address(router).code);
    }
}
