use super::errors::UniswapV3MathError;
use qilin_cfmms::batch_requests;
use qilin_cfmms::batch_requests::uniswap_v3::UniswapV3TickData;

use cfmms::errors::CFMMError;
use cfmms::pool::uniswap_v3::UniswapV3Pool;
use ethers::providers::{Provider, Ws};
use ethers::types::{I256, U256};
use std::sync::Arc;
use uniswap_v3_math::tick_math;

pub const MIN_SQRT_RATIO: U256 = U256([4295128739, 0, 0, 0]);
pub const MAX_SQRT_RATIO: U256 = U256([6743328256752651558, 17280870778742802505, 4294805859, 0]);

#[derive(Default, Debug, Clone)]
pub struct Step {
    pub sqrt_price_start_x96: U256,
    pub tick_next: i32,
    pub initialized: bool,
    pub sqrt_price_next_x96: U256,
    pub amount_in: U256,
    pub amount_out: U256,
    pub fee_amount: U256,
}

#[derive(Debug, Clone)]
pub struct CurrentState {
    amount_specified_remaining: I256,
    amount_calculated: I256,
    sqrt_price_x96: U256,
    tick: i32,
    liquidity: u128,
}

pub struct Tick {
    pub liquidity_gross: u128,
    pub liquidity_net: i128,
    pub fee_growth_outside_0_x_128: U256,
    pub fee_growth_outside_1_x_128: U256,
    pub tick_cumulative_outside: U256,
    pub seconds_per_liquidity_outside_x_128: U256,
    pub seconds_outside: u32,
    pub initialized: bool,
}

pub async fn get_pool_data<M>(
    uniswapv3_pool: UniswapV3Pool,
    zero_for_one: bool,
    provider: Arc<M>,
) -> Result<
    (
        u128,
        u128,
        u32,
        u128,
        U256,
        i32,
        Vec<UniswapV3TickData>,
        i128,
    ),
    CFMMError<Provider<Ws>>,
>
where
    M: ethers::providers::Middleware + 'static,
{
    let fee = uniswapv3_pool.fee;
    let sqrt_price = uniswapv3_pool.sqrt_price;
    let mut tick = uniswapv3_pool.tick;
    let liquidity = uniswapv3_pool.liquidity;
    let (reserver_0, reserve_1) = uniswapv3_pool.calculate_virtual_reserves().unwrap();

    let mut tick_data = Vec::new();
    while tick_data.len() < 500 {
        if let Ok((_tick_data, _)) =
            batch_requests::uniswap_v3::get_uniswap_v3_tick_data_batch_request(
                &uniswapv3_pool,
                tick,
                zero_for_one,
                // TODO: increase num_ticks iteratively
                200,
                None,
                provider.clone(),
            )
            .await
        {
            tick += 200;
            tick_data.extend(_tick_data);
        } else {
            return Err(CFMMError::NoInitializedTicks);
        };
    }

    let liquidity_net = uniswapv3_pool
        .get_liquidity_net(tick, provider.clone())
        .await
        .unwrap();

    Ok((
        reserver_0,
        reserve_1,
        fee,
        liquidity,
        sqrt_price,
        tick,
        tick_data,
        liquidity_net,
    ))
}

pub fn get_tokens_in_from_tokens_out(
    token0_out: Option<f64>,
    token1_out: Option<f64>,
    tick: &i32,
    sqrt_price: &U256,
    liquidity: &u128,
    liquidity_net: i128,
    tick_data: &Vec<UniswapV3TickData>,
    fee: &u32,
) -> Result<f64, UniswapV3MathError> {
    match token0_out {
        Some(val) => {
            if token1_out.is_some() {
                return Err("Cannot take two tokens").unwrap();
            };
            if let Ok((amt_0, _, _, _, _)) = swap(
                -val,
                tick,
                sqrt_price,
                liquidity,
                tick_data,
                liquidity_net,
                &false,
                fee,
            ) {
                return Ok(amt_0);
            } else {
                return Err(UniswapV3MathError::SwapSimulationError);
            }
        }
        None => match token1_out {
            Some(val) => {
                if let Ok((_, amt_1, _, _, _)) = swap(
                    -val,
                    tick,
                    sqrt_price,
                    liquidity,
                    tick_data,
                    liquidity_net,
                    &true,
                    fee,
                ) {
                    return Ok(amt_1);
                } else {
                    return Err(UniswapV3MathError::SwapSimulationError);
                }
            }
            None => Err("At least one token needs to be provided").unwrap(),
        },
    }
}

pub fn get_tokens_out_from_tokens_in(
    token0_in: Option<f64>,
    token1_in: Option<f64>,
    tick: &i32,
    sqrt_price: &U256,
    liquidity: &u128,
    liquidity_net: i128,
    tick_data: &Vec<UniswapV3TickData>,
    fee: &u32,
) -> Result<f64, UniswapV3MathError> {
    match token0_in {
        Some(val) => {
            if token1_in.is_some() {
                return Err("Cannot take two tokens").unwrap();
            };
            if let Ok((_, amt1, _, _, _)) = swap(
                val,
                tick,
                sqrt_price,
                liquidity,
                tick_data,
                liquidity_net,
                &true,
                fee,
            ) {
                return Ok(amt1);
            } else {
                return Err(UniswapV3MathError::SwapSimulationError);
            }
        }
        None => match token1_in {
            Some(val) => {
                if let Ok((amt0, _, _, _, _)) = swap(
                    val,
                    tick,
                    sqrt_price,
                    liquidity,
                    tick_data,
                    liquidity_net,
                    &false,
                    fee,
                ) {
                    return Ok(amt0);
                } else {
                    return Err(UniswapV3MathError::SwapSimulationError);
                }
            }
            None => Err("At least one token needs to be provided").unwrap(),
        },
    }
}

// function assumes getting exact amount out
#[allow(unused_assignments)]
fn swap(
    amount_in: f64,
    tick: &i32,
    sqrt_price_x96: &U256,
    liquidity: &u128,
    tick_data: &Vec<UniswapV3TickData>,
    mut liquidity_net: i128,
    zero_for_one: &bool,
    fee: &u32,
) -> Result<(f64, f64, U256, u128, i32), UniswapV3MathError> {
    let mut tick_data_iter = tick_data.iter();

    let mut state = CurrentState {
        amount_specified_remaining: I256::from(amount_in as i128),
        amount_calculated: I256::from(0),
        sqrt_price_x96: *sqrt_price_x96,
        tick: *tick,
        liquidity: *liquidity,
    };

    let sqrt_price_limit_x96 = if *zero_for_one {
        MIN_SQRT_RATIO + 1
    } else {
        MAX_SQRT_RATIO - 1
    };

    let exact_input = amount_in > 0.0 as f64;

    while state.amount_specified_remaining != I256::zero()
        && state.sqrt_price_x96 != sqrt_price_limit_x96
    {
        let mut step = Step {
            sqrt_price_start_x96: state.sqrt_price_x96,
            ..Default::default()
        };

        let next_tick_data = if let Some(tick_data) = tick_data_iter.next() {
            tick_data
        } else {
            // currently return if tick_data is exhausted
            // later should add a function to return a HashMap that represents the tick_bitmap
            return Err(UniswapV3MathError::TickDataError);
        };

        // TODO: add a tick_bitmap finder like balanceOf slot_finder to use here
        // let mut keep_searching = true;
        // while keep_searching {
        //     match tick_bitmap::next_initialized_tick_within_one_word(
        //         tick_bitmap,
        //         state.tick,
        //         tick_spacing,
        //         zero_for_one,
        //     ) {
        //         Ok((tick_next, initialized)) => {
        //             step.tick_next = tick_next.clamp(tick_math::MIN_TICK, tick_math::MAX_TICK);
        //             step.sqrt_price_next_x96 = tick_math::get_sqrt_ratio_at_tick(step.tick_next)?;
        //             step.initialized = initialized;
        //             if initialized {
        //                 keep_searching = false;
        //             };
        //         }
        //         Err(e) => return Err(Box::new(e)),
        //     };
        // }

        step.tick_next = next_tick_data.tick;

        // prevent overshooting
        if step.tick_next < tick_math::MIN_TICK {
            step.tick_next = tick_math::MIN_TICK;
        } else if step.tick_next > tick_math::MAX_TICK {
            step.tick_next = tick_math::MAX_TICK;
        };

        step.sqrt_price_next_x96 =
            match uniswap_v3_math::tick_math::get_sqrt_ratio_at_tick(step.tick_next) {
                Ok(val) => val,
                Err(_e) => return Err(UniswapV3MathError::TickDataError),
            };

        match uniswap_v3_math::swap_math::compute_swap_step(
            state.sqrt_price_x96,
            if (*zero_for_one && step.sqrt_price_next_x96 < sqrt_price_limit_x96)
                || (!*zero_for_one && step.sqrt_price_next_x96 > sqrt_price_limit_x96)
            {
                sqrt_price_limit_x96
            } else {
                step.sqrt_price_next_x96
            },
            state.liquidity,
            state.amount_specified_remaining,
            *fee,
        ) {
            Ok((sqrt_price_x96, amount_in, amount_out, fee_amount)) => {
                state.sqrt_price_x96 = sqrt_price_x96;
                step.amount_in = amount_in;
                step.amount_out = amount_out;
                step.fee_amount = fee_amount;
            }

            Err(_) => return Err(UniswapV3MathError::StepComputationError),
        }
        println!("step: {:?}", step);

        if exact_input {
            state.amount_specified_remaining = state
                .amount_specified_remaining
                .overflowing_sub(I256::from_raw(
                    step.amount_in.overflowing_add(step.fee_amount).0,
                ))
                .0;
            state.amount_calculated += I256::from_raw(step.amount_out);
        } else {
            state.amount_specified_remaining = state
                .amount_specified_remaining
                .overflowing_add(I256::from_raw(step.amount_out))
                .0;
            state.amount_calculated = state
                .amount_calculated
                .overflowing_add(I256::from_raw(
                    step.amount_in.overflowing_add(step.fee_amount).0,
                ))
                .0;
        }
        println!("state: {:?}", state);

        // shift tick if we reached the next price
        if state.sqrt_price_x96 == step.sqrt_price_next_x96 {
            // if the tick is initialized, run the tick transition
            if next_tick_data.initialized {
                liquidity_net = next_tick_data.liquidity_net;

                if *zero_for_one {
                    liquidity_net = -liquidity_net;
                }

                state.liquidity = if liquidity_net < 0 {
                    state.liquidity - (-liquidity_net as u128)
                } else {
                    state.liquidity + (liquidity_net as u128)
                };
            };

            state.tick = if *zero_for_one {
                step.tick_next.wrapping_sub(1)
            } else {
                step.tick_next
            };
        } else if state.sqrt_price_x96 != step.sqrt_price_start_x96 {
            state.tick = match tick_math::get_tick_at_sqrt_ratio(state.sqrt_price_x96) {
                Ok(val) => val,
                Err(_e) => return Err(UniswapV3MathError::TickDataError),
            };
        };
        println!("state: {:?}", state);
    }

    let (amount0, amount1) = if *zero_for_one == exact_input {
        (
            I256::from(amount_in as i128) - state.amount_specified_remaining,
            state.amount_calculated,
        )
    } else {
        (
            state.amount_calculated,
            I256::from(amount_in as i128) - state.amount_specified_remaining,
        )
    };

    Ok((
        amount0.as_i128() as f64,
        amount1.as_i128() as f64,
        state.sqrt_price_x96,
        state.liquidity,
        state.tick,
    ))
}

#[cfg(test)]
mod test {

    use super::*;
    use crate::arb::f64_2_u256;
    use crate::arb::v3::swap::{get_pool_data, get_tokens_out_from_tokens_in};
    use dotenv::dotenv;
    use env_logger;
    use env_logger::Env;
    use ethers::{
        contract::abigen,
        core::utils::{Anvil, AnvilInstance},
        middleware::SignerMiddleware,
        prelude::LocalWallet,
        providers::{Http, Middleware, Provider},
        types::{H160, U256},
        utils::parse_units,
    };
    use eyre::Result;
    use log;
    use std::env;

    use qilin_cfmms::bindings::{
        uniswap_v3_router_1::uni_v3_swap_router_1_contract, usdt::usdt_contract,
        weth::weth_contract,
    };
    use qilin_cfmms::pool::{Pool, PoolType, PoolVariant};

    abigen! {
        V3_POOL,
        "./src/arb/abi/uniswap_v3_weth_usdt_lp_0_05.json",
        event_derives(serde::Deserialize, serde::Serialize)
    }

    async fn setup() -> Result<(Arc<Provider<Http>>, AnvilInstance)> {
        env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

        dotenv().ok();
        let mainnet_http_url = env::var("HTTP_RPC").unwrap_or_else(|e| {
            log::error!("Error: {}", e);
            return e.to_string();
        });

        let temp_provider = Provider::<Http>::try_from(mainnet_http_url.clone()).unwrap();
        let latest_block = temp_provider.get_block_number().await.unwrap();
        drop(temp_provider);

        let port = 8545u16;
        let url = format!("http://localhost:{}", port).to_string();

        // setup anvil instance for testing
        // note: spawn() will panic if spawn is called without anvil being available in the user’s $PATH
        let anvil = Anvil::new()
            .port(port)
            .fork(mainnet_http_url.clone())
            .fork_block_number(latest_block.as_u64())
            .spawn();

        let provider = Arc::new(
            Provider::<Http>::try_from(url.clone())
                .ok()
                .ok_or(eyre::eyre!("Error connecting to anvil instance"))?,
        );
        log::info!("Connected to anvil instance at {}", url);

        Ok((provider, anvil))
    }

    #[tokio::test]
    async fn test_v3_swap() -> Result<()> {
        let (anvil_provider, _anvil) = setup().await.unwrap();
        let wallet = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
            .parse::<LocalWallet>()?;
        let client = Arc::new(SignerMiddleware::new(anvil_provider.clone(), wallet));

        let weth_instance = weth_contract::weth::new(
            "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse::<H160>()?,
            client.clone(),
        );
        let weth_decimals = weth_instance.decimals().call().await?;

        let usdt_instance = usdt_contract::usdt::new(
            "0xdAC17F958D2ee523a2206206994597C13D831ec7".parse::<H160>()?,
            client.clone(),
        );
        let usdt_decimals = usdt_instance.decimals().call().await?;

        let router_instance = uni_v3_swap_router_1_contract::SwapRouter::new(
            "0xE592427A0AEce92De3Edee1F18E0157C05861564".parse::<H160>()?,
            client.clone(),
        );

        let value: U256 = U256::from(parse_units("500.0", "ether").unwrap());
        let address = client.address();

        let _ = weth_instance.deposit().value(value).send().await?.await?;
        log::info!("WETH deposited to {}", address);

        let weth_balance = weth_instance.balance_of(address).call().await?;
        assert_eq!(weth_balance, value);

        let _ = weth_instance
            .approve(
                "0xE592427A0AEce92De3Edee1F18E0157C05861564".parse::<H160>()?,
                U256::MAX,
            )
            .send()
            .await?
            .await?;

        let _ = usdt_instance
            .approve(
                "0xE592427A0AEce92De3Edee1F18E0157C05861564".parse::<H160>()?,
                U256::MAX,
            )
            .send()
            .await?
            .await?;

        let v3_pool = Pool::new(
            anvil_provider.clone(),
            "0x11b815efB8f581194ae79006d24E0d814B7697F6"
                .parse::<H160>()
                .unwrap(),
            "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
                .parse::<H160>()
                .unwrap(),
            "0xdAC17F958D2ee523a2206206994597C13D831ec7"
                .parse::<H160>()
                .unwrap(),
            U256::from(50),
            PoolVariant::UniswapV3,
        )
        .await
        .unwrap();

        let v3_pool = match v3_pool.pool_type {
            PoolType::UniswapV3(pool) => pool,
            _ => panic!("Wrong pool type"),
        };

        let (
            _token0_reserve,
            _token1_reserve,
            fee,
            liquidity,
            sqrt_price_x_96,
            tick,
            tick_data,
            liquidity_net,
        ) = get_pool_data(v3_pool.clone(), true, anvil_provider.clone())
            .await
            .unwrap();

        let amount_in = 5.0;

        let tokens_1_out = get_tokens_out_from_tokens_in(
            Some(amount_in * 10f64.powi(weth_decimals.into())),
            None,
            &tick,
            &sqrt_price_x_96,
            &liquidity,
            liquidity_net,
            &tick_data,
            &fee,
        )
        .unwrap();

        let amount_out = f64_2_u256(tokens_1_out)
            .checked_div(U256::from(10).pow(usdt_decimals))
            .unwrap();

        let input_param = uni_v3_swap_router_1_contract::ExactInputSingleParams {
            token_in: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
                .parse::<H160>()
                .unwrap(),
            token_out: "0xdAC17F958D2ee523a2206206994597C13D831ec7"
                .parse::<H160>()
                .unwrap(),
            fee: 500,
            recipient: address.clone(),
            deadline: U256::MAX,
            amount_in: U256::from(parse_units("5.0", "ether").unwrap()),
            amount_out_minimum: U256::from(0),
            sqrt_price_limit_x96: U256::from(0),
        };

        let _ = router_instance
            .exact_input_single(input_param)
            .send()
            .await?
            .await?;

        let usdt_balance = usdt_instance
            .balance_of(address)
            .call()
            .await?
            .checked_div(U256::from(10).pow(usdt_decimals.into()))
            .unwrap();

        assert_eq!(usdt_balance, amount_out);

        let (
            _token0_reserve,
            _token1_reserve,
            fee,
            liquidity,
            sqrt_price_x_96,
            tick,
            tick_data,
            liquidity_net,
        ) = get_pool_data(v3_pool.clone(), false, anvil_provider.clone())
            .await
            .unwrap();

        let amount_in = 5000.0;
        let tokens_0_out = get_tokens_out_from_tokens_in(
            None,
            Some(amount_in * 10f64.powi(6)),
            &tick,
            &sqrt_price_x_96,
            &liquidity,
            liquidity_net,
            &tick_data,
            &fee,
        )
        .unwrap();

        let input_param = uni_v3_swap_router_1_contract::ExactInputSingleParams {
            token_in: "0xdAC17F958D2ee523a2206206994597C13D831ec7"
                .parse::<H160>()
                .unwrap(),
            token_out: "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
                .parse::<H160>()
                .unwrap(),
            fee: 500,
            recipient: address.clone(),
            deadline: U256::MAX,
            amount_in: U256::from(5000u128 * 10u128.pow(usdt_decimals.as_u32())),
            amount_out_minimum: U256::from(0),
            sqrt_price_limit_x96: U256::from(0),
        };

        let _ = router_instance
            .exact_input_single(input_param)
            .send()
            .await?
            .await?;

        let weth_balance = weth_instance
            .balance_of(address)
            .call()
            .await?
            .checked_div(U256::from(10).pow(U256::from(weth_decimals)))
            .unwrap();

        Ok(())
    }
}
