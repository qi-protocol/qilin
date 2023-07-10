use crate::arb::u256_2_f64;
use cfmms::pool::uniswap_v2::UniswapV2Pool;
use ethers::providers::Middleware;
use ethers::types::U256;
use std::error::Error;
use std::sync::Arc;

pub async fn get_pool_data<M>(pool: UniswapV2Pool, provider: Arc<M>) -> (u128, u128)
where
    M: Middleware + 'static,
{
    let (token0, token1) = pool.get_reserves(provider).await.unwrap();
    (token0, token1)
}

/// takes either token 0 or 1 out, but not both
pub fn get_tokens_out_from_tokens_in(
    token0_in: Option<f64>,
    token1_in: Option<f64>,
    token0_reserve: &f64,
    token1_reserve: &f64,
) -> Result<f64, Box<dyn Error>> {
    match token0_in {
        Some(val) => {
            if token1_in.is_some() {
                return Err("Cannot take two tokens").unwrap();
            };

            if val == 0.0 {
                return Err("token0_in is zero").unwrap();
            };

            let amount_in_with_fee = val * (u256_2_f64(U256::from(997)));
            let numberator = token1_reserve * &amount_in_with_fee;
            let denominator = token0_reserve * u256_2_f64(U256::from(1000)) + &amount_in_with_fee;
            let result = numberator / denominator;
            Ok(result)
        }
        None => match token1_in {
            Some(val) => {
                if val == 0.0 {
                    return Err("token1_in is zero").unwrap();
                };

                let amount_in_with_fee = val * (u256_2_f64(U256::from(997)));
                let numberator = token0_reserve * &amount_in_with_fee;
                let denominator =
                    token1_reserve * u256_2_f64(U256::from(1000)) + &amount_in_with_fee;
                let result = numberator / denominator;
                Ok(result)
            }
            None => Err("At least one token needs to be provided").unwrap(),
        },
    }
}

pub fn get_tokens_in_from_tokens_out(
    token0_out: Option<f64>,
    token1_out: Option<f64>,
    token0_reserve: &f64,
    token1_reserve: &f64,
) -> Result<f64, Box<dyn Error>> {
    match token0_out {
        Some(val) => {
            if token1_out.is_some() {
                return Err("Cannot take two tokens").unwrap();
            };

            if val == 0.0 {
                return Err("token0_out is zero").unwrap();
            };

            let numerator = token1_reserve * u256_2_f64(U256::from(1000)) * val;
            let denominator = (token0_reserve - val) * (u256_2_f64(U256::from(997)));
            let result = numerator / denominator;

            Ok(result)
        }

        None => match token1_out {
            Some(val) => {
                if val == 0.0 {
                    return Err("token1_out is zero").unwrap();
                };

                let numerator = token0_reserve * u256_2_f64(U256::from(1000)) * val;
                let denominator = (token1_reserve - val) * (u256_2_f64(U256::from(997)));
                let result = numerator / denominator;

                Ok(result)
            }
            None => Err("At least one token needs to be provided").unwrap(),
        },
    }
}

#[cfg(test)]
mod test {

    use super::*;
    use dotenv::dotenv;
    use log;
    use std::env;

    use crate::arb::v2::swap::{get_tokens_in_from_tokens_out, get_tokens_out_from_tokens_in};
    use alloy_primitives::{Address, U256 as alloy_U256};
    use alloy_sol_types::{sol, SolCall};
    use env_logger;
    use env_logger::Env;
    use ethers::{
        contract::abigen,
        core::utils::{Anvil, AnvilInstance},
        middleware::SignerMiddleware,
        prelude::LocalWallet,
        providers::{Http, Middleware, Provider},
        signers::Signer,
        types::{
            transaction::eip2718::TypedTransaction, Eip1559TransactionRequest, NameOrAddress, H160,
            U256, U64,
        },
        utils::parse_units,
    };
    use eyre::Result;

    use qilin_cfmms::bindings::{
        uniswap_v2_router_1::uniswap_v_2_router_1_contract, usdt::usdt_contract,
        weth::weth_contract,
    };

    abigen! {
        V2_POOL,
        "./src/arb/abi/iuniswap_v2_pool.json",
        event_derives(serde::Deserialize, serde::Serialize)
    }

    abigen!(
        IUniswapV2Router02,
        r#"[
            swapExactTokensForTokens(uint256 amountIn, uint256 amountOutMin, address[] calldata path, address to, uint256 deadline)
            getAmountsOut(uint amountIn, address[] memory path) public view returns (uint[] memory amounts)
        ]"#,
    );

    sol! {
        #[derive(Debug)]
        function getAmountsOut(uint amountIn, address[] memory path) public view returns (uint[] memory amounts);
        function getAmountsIn(uint amountOut, address[] memory path) public view returns (uint[] memory amounts);
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
    async fn test_v2_swap() -> Result<()> {
        let (anvil_provider, _anvil) = setup().await.unwrap();
        let wallet = "0xac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80"
            .parse::<LocalWallet>()?;
        let client = Arc::new(SignerMiddleware::new(anvil_provider.clone(), wallet));

        // create an instance of WETH smart contract fomr binding
        let weth_instance = weth_contract::weth::new(
            "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse::<H160>()?,
            client.clone(),
        );

        let usdt_instance = usdt_contract::usdt::new(
            "0xdAC17F958D2ee523a2206206994597C13D831ec7".parse::<H160>()?,
            client.clone(),
        );

        let router_instance = uniswap_v_2_router_1_contract::uniswap_v2_router_1::new(
            "0xf164fC0Ec4E93095b804a4795bBe1e041497b92a".parse::<H160>()?,
            client.clone(),
        );

        let value: U256 = U256::from(parse_units("500.0", "ether").unwrap());
        let address = client.address();

        // deposit 500 ETH to get WETH
        let _ = weth_instance.deposit().value(value).send().await?.await?;
        log::info!("WETH deposited to {}", address);

        let weth_balance = weth_instance.balance_of(address).call().await?;
        assert_eq!(weth_balance, value);

        let _ = weth_instance
            .approve(
                "0xf164fC0Ec4E93095b804a4795bBe1e041497b92a".parse::<H160>()?,
                U256::MAX,
            )
            .send()
            .await?
            .await?;

        // Alloy test setup up
        let path = vec![
            // WETH address
            Address::parse_checksummed("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2", None).unwrap(),
            // USDt address
            Address::parse_checksummed("0xdAC17F958D2ee523a2206206994597C13D831ec7", None).unwrap(),
        ];

        let v2_router_get_amount_out = getAmountsOutCall {
            amountIn: alloy_U256::from(1000000u64),
            path: path,
        };

        let call_data = v2_router_get_amount_out.encode();
        let nonce = client.get_transaction_count(address.clone(), None).await?;

        let tx_req = Eip1559TransactionRequest {
            to: Some(NameOrAddress::Address(
                "0xf164fC0Ec4E93095b804a4795bBe1e041497b92a".parse::<H160>()?,
            )),
            from: Some(address),
            data: Some(call_data.into()),
            chain_id: Some(U64::from(1)),
            max_fee_per_gas: Some(U256::from(1000000000000u64)),
            max_priority_fee_per_gas: Some(U256::from(1000000000000u64)),
            gas: Some(U256::from(1000000u64)),
            nonce: Some(nonce),
            value: None,
            access_list: Default::default(),
        };

        let tx_req = TypedTransaction::Eip1559(tx_req);
        let signed_tx = client.signer().sign_transaction(&tx_req).await?;
        let raw_tx = tx_req.rlp_signed(&signed_tx);

        let receipt = client
            .send_raw_transaction(raw_tx)
            .await?
            .log_msg("Transaction broadcasted, pending confirmation")
            .await?
            .unwrap();

        assert_eq!(receipt.status, Some(U64::from(1)));

        // test get_amounts_out call
        let amt_outs_given_in = router_instance
            .get_amounts_out(
                U256::from(parse_units("5.0", "ether").unwrap()),
                vec![
                    "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse::<H160>()?,
                    "0xdAC17F958D2ee523a2206206994597C13D831ec7".parse::<H160>()?,
                ],
            )
            .call()
            .await?;

        let _ = router_instance
            .swap_exact_tokens_for_tokens(
                U256::from(parse_units("5.0", "ether").unwrap()),
                U256::from(parse_units("0.0", "ether").unwrap()),
                vec![
                    "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse::<H160>()?,
                    "0xdAC17F958D2ee523a2206206994597C13D831ec7".parse::<H160>()?,
                ],
                address,
                U256::from(999999999999999u64),
            )
            .send()
            .await?
            .await?
            .unwrap();

        let usdt_balance = usdt_instance.balance_of(address).call().await?;

        assert_eq!(usdt_balance, amt_outs_given_in[1]);

        // clear out USDT balance from the sender
        let _ = usdt_instance
            .transfer(
                "0xddd453864b2C7a56FC934F7F26A4e8c608B1A4a4".parse::<H160>()?,
                usdt_balance,
            )
            .send()
            .await?
            .await?;
        let usdt_balance = usdt_instance.balance_of(address).call().await?;
        assert_eq!(usdt_balance, U256::from(0u64));

        // Test get_tokens_out_from_tokens_in
        let v2_pool = V2_POOL::new(
            "0x0d4a11d5EEaaC28EC3F61d100daF4d40471f1852".parse::<H160>()?,
            client.clone(),
        );

        let reserve = v2_pool.get_reserves().call().await?;

        let amt_in = 1.0;
        let tokens_out = get_tokens_out_from_tokens_in(
            Some(amt_in),
            None,
            &((reserve.0 / 10u128.pow(18)) as f64),
            &((reserve.1 / 10u128.pow(6)) as f64),
        )
        .unwrap();

        let amt_outs_given_in = router_instance
            .get_amounts_out(
                U256::from(parse_units("1.0", "ether").unwrap()),
                vec![
                    "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse::<H160>()?,
                    "0xdAC17F958D2ee523a2206206994597C13D831ec7".parse::<H160>()?,
                ],
            )
            .call()
            .await?;
        let amt_usdt_out = (amt_outs_given_in[1] / 10u128.pow(6)).as_u128();

        // Assuming tiny bit of inaccuracy due to type conversion
        assert_eq!(amt_usdt_out / 10, (tokens_out / 10.0) as u128);

        let reserve = v2_pool.get_reserves().call().await?;

        let amt_in = 100.0;
        let tokens_out = get_tokens_out_from_tokens_in(
            None,
            Some(amt_in),
            &((reserve.0 / 10u128.pow(18)) as f64),
            &((reserve.1 / 10u128.pow(6)) as f64),
        )
        .unwrap();

        let amt_outs_given_in = router_instance
            .get_amounts_out(
                U256::from(100u128 * 10u128.pow(6)),
                vec![
                    "0xdAC17F958D2ee523a2206206994597C13D831ec7".parse::<H160>()?,
                    "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse::<H160>()?,
                ],
            )
            .call()
            .await?;
        let amount_out_f64 = amt_outs_given_in[1].as_u128() as f64;
        let amt_weth_out = amount_out_f64 / 10f64.powf(18 as f64);

        assert_eq!(
            (tokens_out * 1000.0) as u128,
            (amt_weth_out * 1000.0) as u128
        );

        let reserve = v2_pool.get_reserves().call().await?;

        let amt_out = 1.0;
        let tokens_in = get_tokens_in_from_tokens_out(
            Some(amt_out),
            None,
            &((reserve.0 / 10u128.pow(18)) as f64),
            &((reserve.1 / 10u128.pow(6)) as f64),
        )
        .unwrap();

        let amt_in_given_out = router_instance
            .get_amounts_in(
                U256::from(parse_units("1.0", "ether").unwrap()),
                vec![
                    "0xdAC17F958D2ee523a2206206994597C13D831ec7".parse::<H160>()?,
                    "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse::<H160>()?,
                ],
            )
            .call()
            .await?;
        let amt_usdt_in = (amt_in_given_out[0] / 10u128.pow(6)).as_u128();

        // Assuming tiny bit of inaccuracy due to type conversion
        assert_eq!(amt_usdt_in / 10, (tokens_in / 10.0) as u128);

        let amt_out = 100.0;
        let tokens_in = get_tokens_in_from_tokens_out(
            None,
            Some(amt_out),
            &((reserve.0 / 10u128.pow(18)) as f64),
            &((reserve.1 / 10u128.pow(6)) as f64),
        )
        .unwrap();

        let amt_in_given_out = router_instance
            .get_amounts_in(
                U256::from(100u128 * 10u128.pow(6)),
                vec![
                    "0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2".parse::<H160>()?,
                    "0xdAC17F958D2ee523a2206206994597C13D831ec7".parse::<H160>()?,
                ],
            )
            .call()
            .await?;
        let amount_in_f64 = amt_in_given_out[0].as_u128() as f64;
        let amt_weth_in = amount_in_f64 / 10f64.powf(18 as f64);

        assert_eq!((tokens_in * 1000.0) as u128, (amt_weth_in * 1000.0) as u128);

        Ok(())
    }
}
