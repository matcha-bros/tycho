// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import { Test } from "forge-std/Test.sol";
import { AquaSwapVMSwapAdapter } from "../src/AquaSwapVMSwapAdapter.sol";
import { MockAquaSwapVMRouter } from "../src/MockAquaSwapVMRouter.sol";

contract AquaSwapVMSwapAdapterHarness is AquaSwapVMSwapAdapter {
    function setPool(
        bytes32 poolId,
        address router,
        address maker,
        uint256 traits,
        bytes calldata data
    ) external {
        pools[poolId] = PoolConfig({
            router: router,
            maker: maker,
            traits: traits,
            data: data
        });
    }
}

contract AquaSwapVMSwapAdapterTest is Test {
    AquaSwapVMSwapAdapterHarness adapter;
    MockAquaSwapVMRouter router;

    bytes32 poolId = keccak256("aqua-swapvm-pool");
    address maker = address(0x1234);
    address tokenIn = address(0xA0);
    address tokenOut = address(0xB0);
    uint256 traits = 42;
    bytes orderData = hex"010203040506";

    function setUp() public {
        adapter = new AquaSwapVMSwapAdapterHarness();
        router = new MockAquaSwapVMRouter();
        adapter.setPool(poolId, address(router), maker, traits, orderData);
    }

    function testSwapQuotesThroughConfiguredRouter() public {
        AquaSwapVMSwapAdapter.Trade memory trade =
            adapter.swap(poolId, tokenIn, tokenOut, 0, 5 ether);

        assertEq(trade.receivedAmount, 10 ether);
        assertEq(trade.price.numerator, 10 ether);
        assertEq(trade.price.denominator, 5 ether);
        assertEq(router.lastMaker(), maker);
        assertEq(router.lastTraits(), traits);
        assertEq(router.lastData(), orderData);
        assertEq(router.lastTokenIn(), tokenIn);
        assertEq(router.lastTokenOut(), tokenOut);
        assertEq(router.lastAmount(), 5 ether);
        assertEq(router.lastTakerTraitsAndData(), hex"00000000000000000000000000000000000000000001");
    }

    function testPriceQuotesAllAmounts() public {
        uint256[] memory amounts = new uint256[](2);
        amounts[0] = 1 ether;
        amounts[1] = 3 ether;

        AquaSwapVMSwapAdapter.Fraction[] memory prices =
            adapter.price(poolId, tokenIn, tokenOut, amounts);

        assertEq(prices[0].numerator, 2 ether);
        assertEq(prices[0].denominator, 1 ether);
        assertEq(prices[1].numerator, 6 ether);
        assertEq(prices[1].denominator, 3 ether);
    }

    function testCapabilitiesAndLimitsMatchTychoVmAbi() public {
        uint256[] memory capabilities = adapter.getCapabilities(poolId, tokenIn, tokenOut);
        assertEq(capabilities.length, 3);
        assertEq(capabilities[0], 1);
        assertEq(capabilities[1], 3);
        assertEq(capabilities[2], 7);

        uint256[] memory limits = adapter.getLimits(poolId, tokenIn, tokenOut);
        assertEq(limits.length, 2);
        assertEq(limits[0], 1e18);
        assertEq(limits[1], 1e18);
    }
}
