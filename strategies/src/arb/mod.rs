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
                ) = v3::swap::get_pool_data(uni_v3_pool, borrow_0_buy_1, provider.clone())
                    .await
                    .unwrap();

                cost = ArbPool::new(
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
                let (borrowing_pool_reserve_0, borrowing_pool_reserve_1) =
                    v2::swap::get_pool_data(uni_v2_pool, provider.clone()).await;

                cost = ArbPool::new(
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
                let (repay_pool_reserve_0, repay_pool_reserve_1) =
                    v2::swap::get_pool_data(uni_v2_pool, provider.clone()).await;

                cost.repay_pool_reserve_0 = repay_pool_reserve_0 as f64;
                cost.repay_pool_reserve_1 = repay_pool_reserve_1 as f64;
            }
        };

        let mut _solver = BrentOpt::new(0.0, 0.0);

        match borrow_0_buy_1 {
            true => {
                _solver = BrentOpt::new(1 as f64, cost.borrowing_pool_reserve_0);
            }
            false => {
                _solver = BrentOpt::new(1 as f64, cost.borrowing_pool_reserve_1);
            }
        }

        let init_param = 0.025 as f64 * cost.borrowing_pool_reserve_0;

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
                _debt = v3::swap::get_tokens_out_from_tokens_in(
                    Some(*borrow_amt),
                    None,
                    &borrowing_pool_tick.unwrap(),
                    &borrowing_pool_sqrt_price.unwrap(),
                    &borrowing_pool_liquidity.unwrap(),
                    borrowing_pool_liquidity_net.unwrap(),
                    borrowing_pool_tick_data.as_ref().unwrap(),
                    &borrowing_pool_fee.unwrap(),
                )
                .unwrap()
            }
            false => {
                _debt = v3::swap::get_tokens_out_from_tokens_in(
                    None,
                    Some(*borrow_amt),
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
                _repay = v3::swap::get_tokens_in_from_tokens_out(
                    None,
                    Some(*borrow_amt),
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
                _repay = v3::swap::get_tokens_in_from_tokens_out(
                    Some(*borrow_amt),
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
pub fn q64_2_f64(x: u128) -> f64 {
    let decimals = ((x & 0xFFFFFFFFFFFFFFFF_u128) >> 48) as u32;
    let integers = ((x >> 64) & 0xFFFF) as u32;

    ((integers << 16) + decimals) as f64 / 2_f64.powf(16.0)
}
#[cfg(test)]
mod test {
    use std::env;

    use super::*;
    use dotenv::dotenv;
    use env_logger::Env;
    use ethers::{
        core::types::{H160, U256},
        providers::{Provider, Ws},
    };
    use qilin_cfmms::pool::{Pool, PoolType, PoolVariant};

    pub const USDC_ADDRESS: &str = "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48";
    pub const WETH_ADDRESS: &str = "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2";
    pub const UNISWAP_V2_WETH_USDC_LP: &str = "0xB4e16d0168e52d35CaCD2c6185b44281Ec28C9Dc";
    pub const UNISWAP_V3_WETH_USDC_LP_0_01: &str = "0xE0554a476A092703abdB3Ef35c80e0D76d32939F";

    #[tokio::test]
    async fn test_calc_optimal_arb() {
        env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
        dotenv().ok();
        let mainnet_url = env::var("WSS_RPC").unwrap_or_else(|e| {
            log::error!("Error: {}", e);
            return e.to_string();
        });
        let provider = Arc::new(Provider::<Ws>::connect(mainnet_url.as_str()).await.unwrap());

        let v2_pool = Pool::new(
            provider.clone(),
            UNISWAP_V2_WETH_USDC_LP.parse::<H160>().unwrap(),
            WETH_ADDRESS.parse::<H160>().unwrap(),
            USDC_ADDRESS.parse::<H160>().unwrap(),
            U256::from(300),
            PoolVariant::UniswapV2,
        )
        .await
        .unwrap();

        let v3_pool = Pool::new(
            provider.clone(),
            UNISWAP_V3_WETH_USDC_LP_0_01.parse::<H160>().unwrap(),
            WETH_ADDRESS.parse::<H160>().unwrap(),
            USDC_ADDRESS.parse::<H160>().unwrap(),
            U256::from(10),
            PoolVariant::UniswapV3,
        )
        .await
        .unwrap();

        let (amt, max_profit) =
            ArbPool::calc_optimal_arb(provider.clone(), &v2_pool, &v3_pool, true).await;

        let mut token0_reserve: u128 = 0;
        match v3_pool.pool_type {
            PoolType::UniswapV3(v3_p) => {
                (token0_reserve, _) = v3_p.calculate_virtual_reserves().unwrap();
            }
            _ => {}
        }

        log::info!("Optimal Borrowing Amount: {}", amt);
        log::info!("Max Profit: {}", -max_profit);
        log::info!("Token0 Reserve: {}", token0_reserve);
    }
}
