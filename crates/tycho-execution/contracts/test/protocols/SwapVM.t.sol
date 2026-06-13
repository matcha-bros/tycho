pragma solidity ^0.8.26;

import {Test} from "forge-std/Test.sol";
import {ERC20} from "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import {Math} from "@openzeppelin/contracts/utils/math/Math.sol";
import {TychoRouter, ClientFeeParams} from "@src/TychoRouter.sol";
import {FeeCalculator} from "@src/FeeCalculator.sol";
import {
    IAquaSwapVMRouter,
    SwapVMExecutor
} from "@src/executors/SwapVMExecutor.sol";
import {TransferManager} from "@src/TransferManager.sol";

contract LocalERC20 is ERC20 {
    constructor(string memory name, string memory symbol) ERC20(name, symbol) {}

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }
}

contract SwapVMExecutorTest is Test {
    address private constant ALICE = address(0xa11ce);
    address private constant MAKER = address(0xb0b);
    address private constant ADMIN = address(0xad);
    address private constant PERMIT2 = address(0x100);
    address private constant REAL_AQUA =
        0x5aAdFB43eF8dAF45DD80F4676345b7676f1D70e3;
    address private constant SWAP_VM_ROUTER =
        0x2000000000000000000000000000000000000002;
    uint256 private constant MAKER_TRAITS_USE_AQUA = uint256(1) << 254;
    uint256 private constant ONE = 1e18;
    bytes4 private constant INVALID_DATA_LENGTH =
        bytes4(keccak256("SwapVMExecutor__InvalidDataLength()"));
    bytes4 private constant UNSUPPORTED_VERSION =
        bytes4(keccak256("SwapVMExecutor__UnsupportedVersion(uint8)"));

    TychoRouter private tychoRouter;
    SwapVMExecutor private executor;
    LocalERC20 private tokenIn;
    LocalERC20 private tokenOut;

    function setUp() public {
        vm.etch(PERMIT2, hex"00");
        address placeholderFeeCalculator = address(123);
        vm.etch(placeholderFeeCalculator, hex"00");

        tychoRouter = new TychoRouter(
            PERMIT2, placeholderFeeCalculator, ADMIN, ADMIN, ADMIN, ADMIN
        );
        executor = new SwapVMExecutor();

        address[] memory executors = new address[](1);
        executors[0] = address(executor);
        vm.prank(ADMIN);
        tychoRouter.setExecutors(executors);
        vm.warp(block.timestamp + tychoRouter.DELAY_EXECUTOR_ACTIVATION());

        FeeCalculator feeCalculator = new FeeCalculator(ADMIN);
        vm.prank(ADMIN);
        tychoRouter.setFeeCalculator(address(feeCalculator));
        vm.warp(block.timestamp + tychoRouter.DELAY_FEE_CALCULATOR_ACTIVATION());
        vm.prank(ADMIN);
        tychoRouter.activateFeeCalculator();

        vm.etch(REAL_AQUA, vm.readFileBinary("./test/assets/Aqua.evm.runtime"));
        vm.etch(
            SWAP_VM_ROUTER,
            vm.readFileBinary("./test/assets/AquaSwapVMRouter.evm.runtime")
        );

        tokenIn = new LocalERC20("Token In", "TIN");
        tokenOut = new LocalERC20("Token Out", "TOUT");
    }

    function testGetTransferData() public view {
        bytes memory orderData = hex"1100";
        bytes memory payload =
            _encodePayload(address(tokenIn), address(tokenOut), orderData);

        (
            TransferManager.TransferType transferType,
            address receiver,
            address decodedTokenIn,
            address decodedTokenOut,
            bool outputToRouter
        ) = executor.getTransferData(payload);

        assertEq(
            uint256(transferType),
            uint256(TransferManager.TransferType.ProtocolWillDebit)
        );
        assertEq(receiver, SWAP_VM_ROUTER);
        assertEq(decodedTokenIn, address(tokenIn));
        assertEq(decodedTokenOut, address(tokenOut));
        assertTrue(outputToRouter);
    }

    function testRejectsMalformedPayload() public {
        vm.expectRevert(INVALID_DATA_LENGTH);
        executor.getTransferData(hex"01");
    }

    function testRejectsUnsupportedVersion() public {
        bytes memory payload =
            _encodePayload(address(tokenIn), address(tokenOut), hex"1100");
        payload[0] = bytes1(uint8(2));

        vm.expectRevert(abi.encodeWithSelector(UNSUPPORTED_VERSION, uint8(2)));
        executor.getTransferData(payload);
    }

    function testTychoRouterExecutesRealAquaSwapVMXYCSwap() public {
        uint256 balanceIn = 1_000_000;
        uint256 balanceOut = 2_000_000;
        uint256 amountIn = 100_000;
        bytes memory orderData = hex"1100";

        uint256 expected = _xycAmountOut(amountIn, balanceIn, balanceOut);
        uint256 amountOut = _executeThroughTychoRouter(
            orderData, balanceIn, balanceOut, amountIn, expected
        );

        assertEq(amountOut, expected);
        assertEq(tokenOut.balanceOf(ALICE), expected);
        assertEq(tokenIn.balanceOf(ALICE), 0);
        _assertAquaBalance(address(tokenIn), orderData, balanceIn + amountIn);
        _assertAquaBalance(address(tokenOut), orderData, balanceOut - expected);
    }

    function testTychoRouterExecutesRealAquaSwapVMXYCConcentrate() public {
        uint256 balanceIn = 1_000_000;
        uint256 balanceOut = 2_000_000;
        uint256 amountIn = 100_000;
        uint256 sqrtPriceMin = ONE;
        uint256 sqrtPriceMax = 2 * ONE;
        bytes memory orderData =
            abi.encodePacked(uint8(18), uint8(64), sqrtPriceMin, sqrtPriceMax);

        uint256 expected = _xycConcentrateAmountOut(
            address(tokenIn),
            address(tokenOut),
            amountIn,
            balanceIn,
            balanceOut,
            sqrtPriceMin,
            sqrtPriceMax
        );
        uint256 amountOut = _executeThroughTychoRouter(
            orderData, balanceIn, balanceOut, amountIn, expected
        );

        assertEq(amountOut, expected);
        assertEq(tokenOut.balanceOf(ALICE), expected);
        _assertAquaBalance(address(tokenIn), orderData, balanceIn + amountIn);
        _assertAquaBalance(address(tokenOut), orderData, balanceOut - expected);
    }

    function _executeThroughTychoRouter(
        bytes memory orderData,
        uint256 balanceIn,
        uint256 balanceOut,
        uint256 amountIn,
        uint256 minAmountOut
    ) private returns (uint256 amountOut) {
        _seedAquaBalances(orderData, balanceIn, balanceOut);

        tokenIn.mint(ALICE, amountIn);
        tokenOut.mint(MAKER, balanceOut);

        vm.prank(MAKER);
        tokenOut.approve(REAL_AQUA, type(uint256).max);

        bytes memory payload =
            _encodePayload(address(tokenIn), address(tokenOut), orderData);
        bytes memory swap = abi.encodePacked(address(executor), payload);

        vm.startPrank(ALICE);
        tokenIn.approve(address(tychoRouter), amountIn);
        amountOut = tychoRouter.singleSwap(
            amountIn,
            address(tokenIn),
            address(tokenOut),
            minAmountOut,
            ALICE,
            _noClientFee(),
            swap
        );
        vm.stopPrank();
    }

    function _encodePayload(
        address input,
        address output,
        bytes memory orderData
    ) private pure returns (bytes memory) {
        return abi.encodePacked(
            uint8(1),
            SWAP_VM_ROUTER,
            MAKER,
            bytes32(MAKER_TRAITS_USE_AQUA),
            input,
            output,
            orderData
        );
    }

    function _seedAquaBalances(
        bytes memory orderData,
        uint256 balanceIn,
        uint256 balanceOut
    ) private {
        bytes32 orderHash = _orderHash(orderData);
        vm.store(
            REAL_AQUA,
            _aquaBalanceSlot(
                MAKER, SWAP_VM_ROUTER, orderHash, address(tokenIn)
            ),
            _aquaBalanceValue(balanceIn, 2)
        );
        vm.store(
            REAL_AQUA,
            _aquaBalanceSlot(
                MAKER, SWAP_VM_ROUTER, orderHash, address(tokenOut)
            ),
            _aquaBalanceValue(balanceOut, 2)
        );
    }

    function _assertAquaBalance(
        address token,
        bytes memory orderData,
        uint256 expected
    ) private {
        bytes32 orderHash = _orderHash(orderData);
        bytes32 stored = vm.load(
            REAL_AQUA, _aquaBalanceSlot(MAKER, SWAP_VM_ROUTER, orderHash, token)
        );
        assertEq(uint248(uint256(stored)), expected);
        assertEq(uint8(uint256(stored) >> 248), 2);
    }

    function _orderHash(bytes memory orderData) private view returns (bytes32) {
        return IAquaSwapVMRouter(SWAP_VM_ROUTER)
            .hash(
                IAquaSwapVMRouter.Order({
                    maker: MAKER, traits: MAKER_TRAITS_USE_AQUA, data: orderData
                })
            );
    }

    function _aquaBalanceSlot(
        address maker,
        address app,
        bytes32 strategyHash,
        address token
    ) private pure returns (bytes32) {
        bytes32 makerSlot = keccak256(abi.encode(maker, uint256(0)));
        bytes32 appSlot = keccak256(abi.encode(app, makerSlot));
        bytes32 strategySlot = keccak256(abi.encode(strategyHash, appSlot));
        return keccak256(abi.encode(token, strategySlot));
    }

    function _aquaBalanceValue(uint256 amount, uint8 tokensCount)
        private
        pure
        returns (bytes32)
    {
        return bytes32((uint256(tokensCount) << 248) | amount);
    }

    function _xycAmountOut(
        uint256 amountIn,
        uint256 balanceIn,
        uint256 balanceOut
    ) private pure returns (uint256) {
        return Math.mulDiv(amountIn, balanceOut, balanceIn + amountIn);
    }

    function _xycConcentrateAmountOut(
        address input,
        address output,
        uint256 amountIn,
        uint256 balanceIn,
        uint256 balanceOut,
        uint256 sqrtPriceMin,
        uint256 sqrtPriceMax
    ) private pure returns (uint256) {
        bool isTokenInLt = input < output;
        uint256 balanceLt = isTokenInLt ? balanceIn : balanceOut;
        uint256 balanceGt = isTokenInLt ? balanceOut : balanceIn;
        uint256 priceDelta = sqrtPriceMax - sqrtPriceMin;
        uint256 beta = Math.mulDiv(balanceLt, sqrtPriceMin, ONE)
            + Math.mulDiv(balanceGt, ONE, sqrtPriceMax);
        uint256 fourAc =
            Math.mulDiv(4 * priceDelta, balanceLt * balanceGt, sqrtPriceMax);
        uint256 liquidity = Math.mulDiv(
            beta + Math.sqrt(beta * beta + fourAc), sqrtPriceMax, 2 * priceDelta
        );

        (uint256 virtualBalanceIn, uint256 virtualBalanceOut) = isTokenInLt
            ? (
                balanceIn
                    + Math.mulDiv(
                        liquidity, ONE, sqrtPriceMax, Math.Rounding.Ceil
                    ),
                balanceOut + Math.mulDiv(liquidity, sqrtPriceMin, ONE)
            )
            : (
                balanceIn
                    + Math.mulDiv(
                        liquidity, sqrtPriceMin, ONE, Math.Rounding.Ceil
                    ),
                balanceOut + Math.mulDiv(liquidity, ONE, sqrtPriceMax)
            );

        return _xycAmountOut(amountIn, virtualBalanceIn, virtualBalanceOut);
    }

    function _noClientFee() private pure returns (ClientFeeParams memory) {
        return ClientFeeParams({
            clientFeeBps: 0,
            clientFeeReceiver: address(0),
            maxClientContribution: 0,
            deadline: 0,
            clientSignature: bytes("")
        });
    }
}
