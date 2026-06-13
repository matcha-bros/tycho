// SPDX-License-Identifier: MIT
pragma solidity 0.8.30;

interface IAquaSwapVMRouter {
    struct Order {
        address maker;
        uint256 traits;
        bytes data;
    }

    function quote(
        Order calldata order,
        address tokenIn,
        address tokenOut,
        uint256 amount,
        bytes calldata takerTraitsAndData
    ) external returns (uint256 amountIn, uint256 amountOut, bytes32 orderHash);
}

contract AquaSwapVMSwapAdapter {
    enum Capability {
        SellSide,
        BuySide,
        PriceFunction,
        FeeOnTransfer,
        ConstantPrice,
        TokenBalanceIndependent,
        ScaledPrice,
        HardLimits,
        MarginalPrice
    }

    struct PoolConfig {
        address router;
        address maker;
        uint256 traits;
        bytes data;
    }

    struct Trade {
        uint256 receivedAmount;
        uint256 gasUsed;
        Fraction price;
    }

    struct Fraction {
        uint256 numerator;
        uint256 denominator;
    }

    mapping(bytes32 poolId => PoolConfig config) internal pools;

    error PoolNotConfigured(bytes32 poolId);
    error BuySideNotSupported();
    error InvalidAmount();

    function price(
        bytes32 poolId,
        address sellToken,
        address buyToken,
        uint256[] calldata sellAmounts
    ) external returns (Fraction[] memory prices) {
        prices = new Fraction[](sellAmounts.length);
        for (uint256 i = 0; i < sellAmounts.length; i++) {
            uint256 amount = sellAmounts[i] == 0 ? 1 : sellAmounts[i];
            (, uint256 amountOut) = _quote(poolId, sellToken, buyToken, amount);
            prices[i] = Fraction({ numerator: amountOut, denominator: amount });
        }
    }

    function swap(
        bytes32 poolId,
        address sellToken,
        address buyToken,
        uint8 side,
        uint256 specifiedAmount
    ) external returns (Trade memory trade) {
        if (side != 0) revert BuySideNotSupported();
        if (specifiedAmount == 0) revert InvalidAmount();

        uint256 gasBefore = gasleft();
        (, uint256 amountOut) = _quote(poolId, sellToken, buyToken, specifiedAmount);
        uint256 gasUsed = gasBefore - gasleft();

        trade = Trade({
            receivedAmount: amountOut,
            gasUsed: gasUsed,
            price: Fraction({ numerator: amountOut, denominator: specifiedAmount })
        });
    }

    function getLimits(bytes32, address, address) external pure returns (uint256[] memory limits) {
        limits = new uint256[](2);
        limits[0] = 1e18;
        limits[1] = 1e18;
    }

    function getCapabilities(bytes32, address, address) external pure returns (uint256[] memory capabilities) {
        capabilities = new uint256[](3);
        capabilities[0] = uint256(Capability.SellSide) + 1;
        capabilities[1] = uint256(Capability.PriceFunction) + 1;
        capabilities[2] = uint256(Capability.ScaledPrice) + 1;
    }

    function _quote(
        bytes32 poolId,
        address sellToken,
        address buyToken,
        uint256 amount
    ) internal returns (uint256 amountIn, uint256 amountOut) {
        PoolConfig storage config = pools[poolId];
        if (config.router == address(0)) revert PoolNotConfigured(poolId);

        (amountIn, amountOut,) = IAquaSwapVMRouter(config.router).quote(
            IAquaSwapVMRouter.Order({
                maker: config.maker,
                traits: config.traits,
                data: config.data
            }),
            sellToken,
            buyToken,
            amount,
            _exactInTakerTraits()
        );
    }

    function _exactInTakerTraits() internal pure returns (bytes memory) {
        return hex"00000000000000000000000000000000000000000001";
    }
}
