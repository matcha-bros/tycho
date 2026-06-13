// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

import { IAquaSwapVMRouter } from "./AquaSwapVMSwapAdapter.sol";

contract MockAquaSwapVMRouter {
    address public lastMaker;
    uint256 public lastTraits;
    bytes public lastData;
    address public lastTokenIn;
    address public lastTokenOut;
    uint256 public lastAmount;
    bytes public lastTakerTraitsAndData;

    function quote(
        IAquaSwapVMRouter.Order calldata order,
        address tokenIn,
        address tokenOut,
        uint256 amount,
        bytes calldata takerTraitsAndData
    ) external returns (uint256 amountIn, uint256 amountOut, bytes32 orderHash) {
        lastMaker = order.maker;
        lastTraits = order.traits;
        lastData = order.data;
        lastTokenIn = tokenIn;
        lastTokenOut = tokenOut;
        lastAmount = amount;
        lastTakerTraitsAndData = takerTraitsAndData;

        amountIn = amount;
        amountOut = amount * 2;
        orderHash = keccak256(abi.encode(order.maker, order.traits, order.data));
    }
}
