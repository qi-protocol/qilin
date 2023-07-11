pub mod v2;
pub mod v3;

use argmin::core::observers::{ObserverMode, SlogLogger};
use argmin::core::{CostFunction, Error, Executor};
use argmin::solver::brent::BrentOpt;
use ethers::types::{I256, U256};
use qilin_cfmms::batch_requests::uniswap_v3::UniswapV3TickData;
use qilin_cfmms::pool::{Pool, PoolType};
use std::sync::Arc;

#[derive(Debug)]
struct ArbPool {
    token0_decimals: u8,
    token1_decimals: u8,
    borrowing_pool_reserve_0: f64,
    borrowing_pool_reserve_1: f64,
    repay_pool_reserve_0: f64,
    repay_pool_reserve_1: f64,
    borrowing_pool_type: PoolType,
    repay_pool_type: PoolType,
    borrow_0_buy_1: bool,
    borrowing_pool_fee: Option<u32>,         // V3 only
    borrowing_pool_liquidity: Option<u128>,  // V3 only
    borrowing_pool_sqrt_price: Option<U256>, // V3 only
    borrowing_pool_tick: Option<i32>,        // V3 only
    borrowing_pool_tick_data: Option<Vec<UniswapV3TickData>>,
    borrowing_pool_liquidity_net: Option<i128>, // V3 only
    repay_pool_fee: Option<u32>,                // V3 only
    repay_pool_liquidity: Option<u128>,         // V3 only
    repay_pool_sqrt_price: Option<U256>,        // V3 only
    repay_pool_tick: Option<i32>,               // V3 only
    repay_pool_tick_data: Option<Vec<UniswapV3TickData>>, // V3 only
    repay_pool_liquidity_net: Option<i128>,     // V3 only
}

impl ArbPool {
    #[allow(dead_code)]
    fn new(
        token0_decimals: u8,
        token1_decimals: u8,
        borrowing_pool_reserve_0: f64,
        borrowing_pool_reserve_1: f64,
        repay_pool_reserve_0: f64,
        repay_pool_reserve_1: f64,
        borrowing_pool_type: PoolType,
        repay_pool_type: PoolType,
        borrow_0_buy_1: bool,
        borrowing_pool_fee: Option<u32>,         // V3 only
        borrowing_pool_liquidity: Option<u128>,  // V3 only
        borrowing_pool_sqrt_price: Option<U256>, // V3 only
        borrowing_pool_tick: Option<i32>,        // V3 only
        borrowing_pool_tick_data: Option<Vec<UniswapV3TickData>>, // V3 only
        borrowing_pool_liquidity_net: Option<i128>, // V3 only
        repay_pool_fee: Option<u32>,             // V3 only
        repay_pool_liquidity: Option<u128>,      // V3 only
        repay_pool_sqrt_price: Option<U256>,     // V3 only
        repay_pool_tick: Option<i32>,            // V3 only
        repay_pool_tick_data: Option<Vec<UniswapV3TickData>>, // V3 only
        repay_pool_liquidity_net: Option<i128>,  // V3 only
    ) -> Self {
        #[allow(clippy::too_many_arguments)]
        Self {
            token0_decimals,
            token1_decimals,
            borrowing_pool_reserve_0,
            borrowing_pool_reserve_1,
            repay_pool_reserve_0,
            repay_pool_reserve_1,
            borrowing_pool_type,
            repay_pool_type,
            borrow_0_buy_1,
            borrowing_pool_fee,
            borrowing_pool_liquidity,
            borrowing_pool_sqrt_price,
            borrowing_pool_tick,
            borrowing_pool_tick_data,
            borrowing_pool_liquidity_net,
            repay_pool_fee,
            repay_pool_liquidity,
            repay_pool_sqrt_price,
            repay_pool_tick,
            repay_pool_tick_data,
            repay_pool_liquidity_net,
        }
    }

    /// Called by arb function to calculate the optimal trade size
    #[allow(dead_code)]
    pub async fn calc_optimal_arb<M>(
        provider: Arc<M>,
        borrowing_pool: &Pool,
        repay_pool: &Pool,
        borrow_0_buy_1: bool,
    ) -> (f64, f64)
    where
        M: ethers::providers::Middleware + 'static,
    {
        let mut cost: ArbPool;
        match borrowing_pool.pool_type {
            PoolType::UniswapV3(uni_v3_pool) => {
                let (
                    borrowing_pool_reserve_0,
                    borrowing_pool_reserve_1,
                    borrowing_pool_fee,
                    borrowing_pool_liquidity,
                    borrowing_pool_sqrt_price,
                    borrowing_pool_tick,
                    borrowing_pool_tick_data,
                    borrowing_pool_liquidity_net,
                    token0_decimals,
                    token1_decimals,
                ) = v3::swap::get_pool_data(uni_v3_pool, borrow_0_buy_1, provider.clone())
                    .await
                    .unwrap();

                cost = ArbPool::new(
                    token0_decimals,
                    token1_decimals,
                    borrowing_pool_reserve_0 as f64,
                    borrowing_pool_reserve_1 as f64,
                    0.0,
                    0.0,
                    borrowing_pool.pool_type,
                    repay_pool.pool_type,
                    borrow_0_buy_1,
                    Some(borrowing_pool_fee),
                    Some(borrowing_pool_liquidity),
                    Some(borrowing_pool_sqrt_price),
                    Some(borrowing_pool_tick),
                    Some(borrowing_pool_tick_data),
                    Some(borrowing_pool_liquidity_net),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                )
            }
            PoolType::UniswapV2(uni_v2_pool) => {
                let (
                    borrowing_pool_reserve_0,
                    borrowing_pool_reserve_1,
                    token0_decimals,
                    token1_decimals,
                ) = v2::swap::get_pool_data(uni_v2_pool, provider.clone()).await;

                cost = ArbPool::new(
                    token0_decimals,
                    token1_decimals,
                    borrowing_pool_reserve_0 as f64,
                    borrowing_pool_reserve_1 as f64,
                    0.0,
                    0.0,
                    borrowing_pool.pool_type,
                    repay_pool.pool_type,
                    borrow_0_buy_1,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                )
            }
        };

        match repay_pool.pool_type {
            PoolType::UniswapV3(uni_v3_pool) => {
                let (
                    repay_pool_reserve_0,
                    repay_pool_reserve_1,
                    repay_pool_fee,
                    repay_pool_liquidity,
                    repay_pool_sqrt_price,
                    repay_pool_tick,
                    repay_pool_tick_data,
                    repay_pool_liquidity_net,
                    _,
                    _,
                ) = v3::swap::get_pool_data(uni_v3_pool, borrow_0_buy_1, provider.clone())
                    .await
                    .unwrap();

                cost.repay_pool_reserve_0 = repay_pool_reserve_0 as f64;
                cost.repay_pool_reserve_1 = repay_pool_reserve_1 as f64;
                cost.repay_pool_fee = Some(repay_pool_fee);
                cost.repay_pool_liquidity = Some(repay_pool_liquidity);
                cost.repay_pool_sqrt_price = Some(repay_pool_sqrt_price);
                cost.repay_pool_tick = Some(repay_pool_tick);
                cost.repay_pool_tick_data = Some(repay_pool_tick_data);
                cost.repay_pool_liquidity_net = Some(repay_pool_liquidity_net);
            }
            PoolType::UniswapV2(uni_v2_pool) => {
                let (repay_pool_reserve_0, repay_pool_reserve_1, _, _) =
                    v2::swap::get_pool_data(uni_v2_pool, provider.clone()).await;

                cost.repay_pool_reserve_0 = repay_pool_reserve_0 as f64;
                cost.repay_pool_reserve_1 = repay_pool_reserve_1 as f64;
            }
        };

        let mut _solver = BrentOpt::new(0.0, 0.0);

        let init_param: f64;
        
        // since reserve from univ3 pool is virtual reserve, which bounded the 
        // range significantly. Here, we'll use repay pool's reserve to bound
        // the searching range if the borrowing pool is univ3
        match borrow_0_buy_1 {
            true => {
                match borrowing_pool.pool_type {
                    PoolType::UniswapV3(_) => {
                        init_param = cost.repay_pool_reserve_0 * 0.025;
                        _solver = BrentOpt::new(1 as f64, cost.repay_pool_reserve_0);
                    }
                    PoolType::UniswapV2(_) => {
                        init_param = cost.borrowing_pool_reserve_0 * 0.025;
                        _solver = BrentOpt::new(1 as f64, cost.borrowing_pool_reserve_0);
                    }
                };
            }
            false => {
                match borrowing_pool.pool_type {
                    PoolType::UniswapV3(_) => {
                        init_param = cost.repay_pool_reserve_1 * 0.025;
                        _solver = BrentOpt::new(1 as f64, cost.repay_pool_reserve_1);
                    }
                    PoolType::UniswapV2(_) => {
                        init_param = cost.borrowing_pool_reserve_1 * 0.025;
                        _solver = BrentOpt::new(1 as f64, cost.borrowing_pool_reserve_1);
                    }

                }
            }
        }

        let executor = Executor::new(cost, _solver);

        let res = executor
            .configure(|state| state.param(init_param))
            .add_observer(SlogLogger::term(), ObserverMode::Always)
            .run()
            .unwrap();

        (res.state().best_param.unwrap(), res.state().best_cost)
    }
}

impl CostFunction for ArbPool {
    type Param = f64;
    type Output = f64;

    fn cost(&self, p: &Self::Param) -> Result<Self::Output, Error> {
        Ok(maximize_arb_profit(
            &p,
            &self.token0_decimals,
            &self.token1_decimals,
            &self.borrowing_pool_reserve_0,
            &self.borrowing_pool_reserve_1,
            &self.repay_pool_reserve_0,
            &self.repay_pool_reserve_1,
            &self.borrow_0_buy_1,
            &self.borrowing_pool_type,
            &self.repay_pool_type,
            &self.borrowing_pool_fee,
            &self.borrowing_pool_liquidity,
            &self.borrowing_pool_sqrt_price,
            &self.borrowing_pool_tick,
            &self.borrowing_pool_tick_data,
            &self.borrowing_pool_liquidity_net,
            &self.repay_pool_fee,
            &self.repay_pool_liquidity,
            &self.repay_pool_sqrt_price,
            &self.repay_pool_tick,
            &self.repay_pool_tick_data,
            &self.repay_pool_liquidity_net,
        ))
    }
}

fn maximize_arb_profit(
    borrow_amt: &f64,
    token0_decimals: &u8,
    token1_decimals: &u8,
    borrowing_pool_reserve_0: &f64,
    borrowing_pool_reserve_1: &f64,
    repay_pool_reserve_0: &f64,
    repay_pool_reserve_1: &f64,
    borrow_0_buy_1: &bool,
    borrowing_pool_type: &PoolType,
    repay_pool_type: &PoolType,
    borrowing_pool_fee: &Option<u32>,         // V3 only
    borrowing_pool_liquidity: &Option<u128>,  // V3 only
    borrowing_pool_sqrt_price: &Option<U256>, // V3 only
    borrowing_pool_tick: &Option<i32>,        // V3 only
    borrowing_pool_tick_data: &Option<Vec<UniswapV3TickData>>, // V3 only
    borrowing_pool_liquidity_net: &Option<i128>, // V3 only
    repay_pool_fee: &Option<u32>,             // V3 only
    repay_pool_liquidity: &Option<u128>,      // V3 only
    repay_pool_sqrt_price: &Option<U256>,     // V3 only
    repay_pool_tick: &Option<i32>,            // V3 only
    repay_pool_tick_data: &Option<Vec<UniswapV3TickData>>, // V3 only
    repay_pool_liquidity_net: &Option<i128>,  // V3 only
) -> f64 {
    let mut _debt: f64 = 0.0;
    let mut _repay: f64 = 0.0;

    match borrowing_pool_type {
        PoolType::UniswapV2(_) => match borrow_0_buy_1 {
            true => {
                _debt = v2::swap::get_tokens_out_from_tokens_in(
                    Some(*borrow_amt),
                    None,
                    borrowing_pool_reserve_0,
                    borrowing_pool_reserve_1,
                )
                .unwrap();
            }
            false => {
                _debt = v2::swap::get_tokens_out_from_tokens_in(
                    None,
                    Some(*borrow_amt),
                    borrowing_pool_reserve_0,
                    borrowing_pool_reserve_1,
                )
                .unwrap();
            }
        },
        PoolType::UniswapV3(_) => match borrow_0_buy_1 {
            true => {
                let borrow_amt = *borrow_amt * 10f64.powi(*token0_decimals as i32);
                _debt = v3::swap::get_tokens_out_from_tokens_in(
                    Some(borrow_amt),
                    None,
                    &borrowing_pool_tick.unwrap(),
                    &borrowing_pool_sqrt_price.unwrap(),
                    &borrowing_pool_liquidity.unwrap(),
                    borrowing_pool_liquidity_net.unwrap(),
                    borrowing_pool_tick_data.as_ref().unwrap(),
                    &borrowing_pool_fee.unwrap(),
                )
                .unwrap();
            }
            false => {
                let borrow_amt = *borrow_amt * 10f64.powi(*token1_decimals as i32);
                _debt = v3::swap::get_tokens_out_from_tokens_in(
                    None,
                    Some(borrow_amt),
                    &borrowing_pool_tick.unwrap(),
                    &borrowing_pool_sqrt_price.unwrap(),
                    &borrowing_pool_liquidity.unwrap(),
                    borrowing_pool_liquidity_net.unwrap(),
                    borrowing_pool_tick_data.as_ref().unwrap(),
                    &borrowing_pool_fee.unwrap(),
                )
                .unwrap()
            }
        },
    }

    match repay_pool_type {
        PoolType::UniswapV2(_) => match borrow_0_buy_1 {
            true => {
                _repay = v2::swap::get_tokens_in_from_tokens_out(
                    None,
                    Some(*borrow_amt),
                    repay_pool_reserve_0,
                    repay_pool_reserve_1,
                )
                .unwrap();
            }
            false => {
                _repay = v2::swap::get_tokens_in_from_tokens_out(
                    Some(*borrow_amt),
                    None,
                    repay_pool_reserve_0,
                    repay_pool_reserve_1,
                )
                .unwrap();
            }
        },
        PoolType::UniswapV3(_) => match borrow_0_buy_1 {
            true => {
                let borrow_amt = *borrow_amt * 10f64.powi(*token1_decimals as i32);
                _repay = v3::swap::get_tokens_in_from_tokens_out(
                    None,
                    Some(borrow_amt),
                    &repay_pool_tick.unwrap(),
                    &repay_pool_sqrt_price.unwrap(),
                    &repay_pool_liquidity.unwrap(),
                    repay_pool_liquidity_net.unwrap(),
                    repay_pool_tick_data.as_ref().unwrap(),
                    &repay_pool_fee.unwrap(),
                )
                .unwrap()
            }
            false => {
                let borrow_amt = *borrow_amt * 10f64.powi(*token0_decimals as i32);
                _repay = v3::swap::get_tokens_in_from_tokens_out(
                    Some(borrow_amt),
                    None,
                    &repay_pool_tick.unwrap(),
                    &repay_pool_sqrt_price.unwrap(),
                    &repay_pool_liquidity.unwrap(),
                    repay_pool_liquidity_net.unwrap(),
                    repay_pool_tick_data.as_ref().unwrap(),
                    &repay_pool_fee.unwrap(),
                )
                .unwrap()
            }
        },
    };

    return -(_debt + _repay);
}

#[allow(dead_code)]
pub(crate) fn u256_2_f64(value: U256) -> f64 {
    value.as_u128() as f64
}

#[allow(dead_code)]
pub(crate) fn f64_2_u256(value: f64) -> U256 {
    U256::from(value as u128)
}

#[allow(dead_code)]
pub(crate) fn f64_2_i256(value: f64) -> I256 {
    I256::from(value as i128)
}

#[allow(dead_code)]
pub(crate) fn i256_2_f64(value: I256) -> f64 {
    let integer_part = (value / I256::from(2).pow(128)).as_i128() as f64;
    let decimal_part = (value % I256::from(2).pow(128)).as_i128() as f64 / (2_f64.powi(128) - 1.0);
    integer_part + decimal_part
}

#[allow(dead_code)]
pub(crate) fn q64_2_f64(x: u128) -> f64 {
    let decimals = ((x & 0xFFFFFFFFFFFFFFFF_u128) >> 48) as u32;
    let integers = ((x >> 64) & 0xFFFF) as u32;

    ((integers << 16) + decimals) as f64 / 2_f64.powf(16.0)
}
#[cfg(test)]
mod test {
    use std::env;

    use super::*;
    use dotenv::dotenv;
    use env_logger;
    use env_logger::Env;
    use ethers::{
        contract::abigen,
        core::types::{H160, U256},
        core::utils::{Anvil, AnvilInstance},
        middleware::SignerMiddleware,
        prelude::LocalWallet,
        providers::{Http, Middleware, Provider},
        utils::parse_units,
    };
    use eyre::Result;
    use qilin_cfmms::bindings::{
        uniswap_v2_router_1::uniswap_v_2_router_1_contract,
        uniswap_v3_router_1::uni_v3_swap_router_1_contract, usdt::usdt_contract,
        weth::weth_contract,
    };
    use qilin_cfmms::pool::{Pool, PoolType, PoolVariant};

    abigen! {
        V2_POOL,
        "./src/arb/abi/iuniswap_v2_pool.json",
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
    async fn test_calc_optimal_arb() -> Result<()> {
        let (anvil_provider, _anvil) = setup().await.unwrap();
        let wallet = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
            .parse::<LocalWallet>()?;
        let client = Arc::new(SignerMiddleware::new(anvil_provider.clone(), wallet));

        let weth_instance = weth_contract::weth::new(
            "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse::<H160>()?,
            client.clone(),
        );

        let usdt_instance = usdt_contract::usdt::new(
            "0xdAC17F958D2ee523a2206206994597C13D831ec7".parse::<H160>()?,
            client.clone(),
        );

        let _v3_router_instance = uni_v3_swap_router_1_contract::SwapRouter::new(
            "0xE592427A0AEce92De3Edee1F18E0157C05861564".parse::<H160>()?,
            client.clone(),
        );

        let _v2_router_instance = uniswap_v_2_router_1_contract::uniswap_v2_router_1::new(
            "0xf164fC0Ec4E93095b804a4795bBe1e041497b92a".parse::<H160>()?,
            client.clone(),
        );

        let value: U256 = U256::from(parse_units("500.0", "ether").unwrap());
        let _address = client.address();

        let _ = weth_instance.deposit().value(value).send().await?.await?;

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
            client.clone(),
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
        let v2_pool = Pool::new(
            client.clone(),
            "0x0d4a11d5EEaaC28EC3F61d100daF4d40471f1852"
                .parse::<H160>()
                .unwrap(),
            "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
                .parse::<H160>()
                .unwrap(),
            "0xdAC17F958D2ee523a2206206994597C13D831ec7"
                .parse::<H160>()
                .unwrap(),
            U256::from(30),
            PoolVariant::UniswapV2,
        )
        .await
        .unwrap();


        let uni_v3_pool = match v3_pool.pool_type {
            PoolType::UniswapV3(pool) => pool,
            _ => panic!("Wrong pool type"),
        };

        let temp_v2_pool = V2_POOL::new(
            "0x0d4a11d5EEaaC28EC3F61d100daF4d40471f1852".parse::<H160>()?,
            client.clone(),
        );
        let v2_reserve = temp_v2_pool.get_reserves().call().await?;

        let uni_v2_pool = match v2_pool.pool_type {
            PoolType::UniswapV2(pool) => pool,
            _ => panic!("Wrong pool type"),
        };

        let v3_amount_out = uni_v3_pool
            .simulate_swap(
                "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
                    .parse::<H160>()
                    .unwrap(),
                U256::from(parse_units("5.0", "ether").unwrap()),
                client.clone(),
            )
            .await
            .unwrap();

        let v2_amount_out = uni_v2_pool
            .simulate_swap(
                "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2"
                    .parse::<H160>()
                    .unwrap(),
                U256::from(parse_units("5.0", "ether").unwrap()),
            );

            let  borrow_pool: Pool;
            let  repay_pool: Pool;
        if v3_amount_out > v2_amount_out {
            borrow_pool = v3_pool;
            repay_pool = v2_pool;
            log::info!("Borrowing from V3");
        } else {
            borrow_pool = v2_pool;
            repay_pool = v3_pool;
            log::info!("Borrowing from V2");
        }

        let (amt, _) =
            ArbPool::calc_optimal_arb(client.clone(), &borrow_pool, &repay_pool, true).await;

        log::info!("Optimal Borrowing Amount: {}", amt);

        assert!(amt < v2_reserve.0 as f64);
        Ok(())
    }
}
