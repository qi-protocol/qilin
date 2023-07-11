use cfmms::{
    dex, pool,
    pool::{uniswap_v2::UniswapV2Pool, uniswap_v3::UniswapV3Pool},
};
use ethers::prelude::*;
use serde::{Deserialize, Serialize};
#[allow(unused_imports)]
use std::mem;
use std::sync::Arc;

pub type PoolVariant = dex::DexVariant;
pub type PoolType = pool::Pool;
type RustyPool = rusty::cfmm::Pool;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub struct Pool {
    pub address: Address,
    pub token_0: Address,
    pub token_1: Address,
    pub swap_fee: U256,
    pub pool_variant: PoolVariant,
    pub pool_type: PoolType, // by adding pool_type, we double the struct size to 248 bytes
}

impl Default for Pool {
    fn default() -> Self {
        Pool {
            address: Address::zero(),
            token_0: Address::zero(),
            token_1: Address::zero(),
            swap_fee: U256::zero(),
            pool_variant: PoolVariant::UniswapV2,
            pool_type: PoolType::UniswapV2(UniswapV2Pool::default()),
        }
    }
}

impl Pool {
    pub fn new_empty_pool(
        address: Address,
        token_a: Address,
        token_b: Address,
        swap_fee: U256,
        pool_variant: PoolVariant,
    ) -> Self {
        let (token_0, token_1) = if token_a < token_b {
            (token_a, token_b)
        } else {
            (token_b, token_a)
        };
        match pool_variant {
            PoolVariant::UniswapV2 => {
                let res = Pool {
                    address,
                    token_0,
                    token_1,
                    swap_fee,
                    pool_variant,
                    pool_type: PoolType::UniswapV2(UniswapV2Pool::new(
                        address, token_0, 0, token_1, 0, 0, 0, 300,
                    )),
                };
                res
            }
            PoolVariant::UniswapV3 => {
                let res = Pool {
                    address,
                    token_0,
                    token_1,
                    swap_fee,
                    pool_variant,
                    pool_type: PoolType::UniswapV3(UniswapV3Pool::new(
                        address,
                        token_0,
                        0,
                        token_1,
                        0,
                        0,
                        0,
                        U256::zero(),
                        0,
                        0,
                        0,
                    )),
                };
                res
            }
        }
    }

    // Creates a new pool instance
    pub async fn new<M>(
        provider: Arc<M>,
        address: Address,
        token_a: Address,
        token_b: Address,
        swap_fee: U256,
        pool_variant: PoolVariant,
    ) -> Option<Pool> 
    where
        M: Middleware + 'static,
    {
        let (token_0, token_1) = if token_a < token_b {
            (token_a, token_b)
        } else {
            (token_b, token_a)
        };
        // TODO: change this to get_pool_data_batch_request, for initial and later updates
        match pool_variant {
            PoolVariant::UniswapV2 => {
                if let Ok(_pool_type) =
                    pool::UniswapV2Pool::new_from_address(address, provider.clone()).await
                {
                    // println!("Getting Uni V2 Pool: {:?}", _pool_type);

                    let res = Pool {
                        address,
                        token_0,
                        token_1,
                        swap_fee,
                        pool_variant,
                        pool_type: PoolType::UniswapV2(_pool_type),
                    };

                    // let size  = mem::size_of_val(&res);
                    // println!("Size of Pool: {:?}", size);
                    Some(res)
                } else {
                    None
                }
            }
            PoolVariant::UniswapV3 => {
                if let Ok(_pool_type) =
                    pool::UniswapV3Pool::new_from_address(address, provider.clone()).await
                {
                    // println!("Getting Uni V3 Pool: {:?}", _pool_type);
                    let res = Pool {
                        address,
                        token_0,
                        token_1,
                        swap_fee,
                        pool_variant,
                        pool_type: PoolType::UniswapV3(_pool_type),
                    };

                    // let size  = mem::size_of_val(&res);
                    // println!("Size of Pool: {:?}", size);
                    Some(res)
                } else {
                    None
                }
            }
        }
    }

    // update single pool state
    pub async fn update_pool_state(&mut self, provider: Arc<Provider<Ws>>) {
        match self.pool_variant {
            PoolVariant::UniswapV2 => {
                if let Ok(_pool_type) =
                    pool::UniswapV2Pool::new_from_address(self.address, provider.clone()).await
                {
                    self.pool_type = PoolType::UniswapV2(_pool_type);
                }
            }
            PoolVariant::UniswapV3 => {
                if let Ok(_pool_type) =
                    pool::UniswapV3Pool::new_from_address(self.address, provider.clone()).await
                {
                    self.pool_type = PoolType::UniswapV3(_pool_type);
                }
            }
        }
    }

    pub fn to_rp(&self) -> RustyPool {
        match self.pool_variant {
            PoolVariant::UniswapV2 => {
                let mut rp: RustyPool = RustyPool::default();
                if let PoolType::UniswapV2(pool_type) = &self.pool_type {
                    rp = RustyPool::from(
                        &self.address,
                        &self.token_0,
                        &self.token_1,
                        &self.swap_fee,
                        &PoolVariant::UniswapV2,
                        &PoolType::UniswapV2(*pool_type),
                    );
                };

                rp
            }
            PoolVariant::UniswapV3 => {
                let mut rp: RustyPool = RustyPool::default();
                if let PoolType::UniswapV3(pool_type) = &self.pool_type {
                    rp = RustyPool::from(
                        &self.address,
                        &self.token_0,
                        &self.token_1,
                        &self.swap_fee,
                        &PoolVariant::UniswapV3,
                        &PoolType::UniswapV3(*pool_type),
                    );
                };

                rp
            }
        }
    }
}
