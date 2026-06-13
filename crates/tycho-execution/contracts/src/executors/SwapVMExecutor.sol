// SPDX-License-Identifier: BUSL-1.1
pragma solidity ^0.8.26;

import {IExecutor} from "@interfaces/IExecutor.sol";
import {TransferManager} from "../TransferManager.sol";

error SwapVMExecutor__InvalidDataLength();
error SwapVMExecutor__UnsupportedVersion(uint8 version);
error SwapVMExecutor__ZeroAddress();

interface IAquaSwapVMRouter {
    struct Order {
        address maker;
        uint256 traits;
        bytes data;
    }

    function swap(
        Order calldata order,
        address tokenIn,
        address tokenOut,
        uint256 amount,
        bytes calldata takerTraitsAndData
    ) external returns (uint256 amountIn, uint256 amountOut, bytes32 orderHash);

    function hash(Order calldata order)
        external
        view
        returns (bytes32 orderHash);
}

/// @title SwapVMExecutor
/// @notice Adapter that lets TychoRouter execute exact-in Aqua-backed SwapVM orders.
/// @dev Payload v1 is:
///      version(1) | aquaSwapVMRouter(20) | maker(20) | orderTraits(32) |
///      tokenIn(20) | tokenOut(20) | orderData(variable).
///      TychoRouter approves the AquaSwapVMRouter as a ProtocolWillDebit
///      spender. The executor then calls the real SwapVM router with fixed
///      exact-in + transferFrom-and-Aqua-push taker traits, so SwapVM pulls
///      input from TychoRouter and returns output to TychoRouter.
contract SwapVMExecutor is IExecutor {
    uint256 private constant _MIN_DATA_LENGTH = 114;
    uint8 private constant _VERSION_1 = 1;

    // TakerTraits header: 20 zero bytes of slice indexes + uint16 flags.
    // Flags: exact-in (0x0001) + useTransferFromAndAquaPush (0x0040).
    bytes private constant _EXACT_IN_AQUA_PUSH_TAKER_TRAITS =
        hex"00000000000000000000000000000000000000000041";

    function fundsExpectedAddress(
        bytes calldata /* data */
    )
        external
        view
        returns (address receiver)
    {
        return msg.sender;
    }

    function swap(uint256 amountIn, bytes calldata data, address)
        external
        payable
    {
        (
            address swapVMRouter,
            address maker,
            uint256 orderTraits,
            address tokenIn,
            address tokenOut,
            bytes calldata orderData
        ) = _decodeData(data);

        IAquaSwapVMRouter(swapVMRouter)
            .swap(
                IAquaSwapVMRouter.Order({
                    maker: maker, traits: orderTraits, data: orderData
                }),
                tokenIn,
                tokenOut,
                amountIn,
                _EXACT_IN_AQUA_PUSH_TAKER_TRAITS
            );
    }

    function getTransferData(bytes calldata data)
        external
        pure
        returns (
            TransferManager.TransferType transferType,
            address receiver,
            address tokenIn,
            address tokenOut,
            bool outputToRouter
        )
    {
        (receiver,,, tokenIn, tokenOut,) = _decodeData(data);
        transferType = TransferManager.TransferType.ProtocolWillDebit;
        outputToRouter = true;
    }

    function _decodeData(bytes calldata data)
        internal
        pure
        returns (
            address swapVMRouter,
            address maker,
            uint256 orderTraits,
            address tokenIn,
            address tokenOut,
            bytes calldata orderData
        )
    {
        if (data.length < _MIN_DATA_LENGTH) {
            revert SwapVMExecutor__InvalidDataLength();
        }

        uint8 version = uint8(data[0]);
        if (version != _VERSION_1) {
            revert SwapVMExecutor__UnsupportedVersion(version);
        }

        swapVMRouter = address(bytes20(data[1:21]));
        maker = address(bytes20(data[21:41]));
        orderTraits = uint256(bytes32(data[41:73]));
        tokenIn = address(bytes20(data[73:93]));
        tokenOut = address(bytes20(data[93:113]));
        orderData = data[113:];

        if (
            swapVMRouter == address(0) || maker == address(0)
                || tokenIn == address(0) || tokenOut == address(0)
        ) {
            revert SwapVMExecutor__ZeroAddress();
        }
    }
}
