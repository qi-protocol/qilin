use crate::utils::constants::{UNISWAP_V3_FACTORY, UNISWAP_V3_WETH_USDT_LP_0_01};
use anyhow::Result;
use dotenv::dotenv;
use ethers::core::types::Chain;
use ethers::etherscan::Client;
use ethers::prelude::Abigen;
use ethers::prelude::*;
use ethers::types::H160;
use log;
use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::prelude::*;

pub async fn generate_abigen(
    client: &Client,
    contract_name: &str,
    contract_address: H160,
) -> Result<()> {
    let metadata = client.contract_source_code(contract_address).await?;
    let abi = metadata.items[0].abi.as_str();
    log::info!("ABI: {:?}", abi);

    let mut file = File::create(format!("../abi/{}.json", contract_name.to_lowercase()))?;
    file.write_all(abi.as_bytes())?;
    log::info!("writing to file: {:?}", contract_name.to_lowercase());

    let abi_source = format!("../abi/{}.json", contract_name.to_lowercase());
    Abigen::new(contract_name.to_lowercase(), abi_source)
        .unwrap()
        .generate()
        .unwrap()
        .write_to_file(format!(
            "../cfmms/bindings/{}.rs",
            contract_name.to_lowercase()
        ))?;

    Ok(())
}

pub async fn generate_abigen_for_addresses() -> Result<(), Box<dyn Error>> {
    dotenv().ok();
    let _etherscan_key = env::var("ETHERSCAN_API_KEY").unwrap();
    let etherscan_client = Client::new(Chain::Mainnet, _etherscan_key).unwrap();

    let mut address_book = HashMap::new();

    // address_book.insert("DAI", DAI_ADDRESS);
    // address_book.insert("USDC", USDC_ADDRESS);
    // address_book.insert("USDT", USDT_ADDRESS);
    // address_book.insert("WETH", WETH_ADDRESS);
    // address_book.insert("UNISWAP_V2_ROUTER_1", UNISWAP_V2_ROUTER_1);
    // address_book.insert("UNISWAP_V2_ROUTER_2", UNISWAP_V2_ROUTER_2);
    // address_book.insert("UNISWAP_V3_ROUTER_1", UNISWAP_V3_ROUTER_1);
    // address_book.insert("UNISWAP_V3_ROUTER_2", UNISWAP_V3_ROUTER_2);
    // address_book.insert("UNISWAP_UNIVERSAL_ROUTER", UNISWAP_UNIVERSAL_ROUTER);
    // address_book.insert("UNISWAP_V3_WETH_DAI_LP", UNISWAP_V3_WETH_DAI_LP);
    // address_book.insert("UNISWAP_V3_QUOTER", UNISWAP_V3_QUOTER);
    // address_book.insert("UNISWAP_V3_QUOTER_V2", UNISWAP_V3_QUOTER_V2);
    // address_book.insert("UNISWAP_V2_FACTORY", UNISWAP_V2_FACTORY);
    // address_book.insert("UNISWAP_V3_FACTORY", UNISWAP_V3_FACTORY);
    address_book.insert("UNISWAP_V3_WETH_USDT_LP_0_01", UNISWAP_V3_WETH_USDT_LP_0_01);

    let mut parsed_addr;
    for (name, addr) in address_book {
        parsed_addr = addr.parse::<H160>()?;
        let _ = generate_abigen(&etherscan_client, name, parsed_addr).await;
    }

    Ok(())
}

pub async fn generate_abigen_for_given_address(_address: Address) -> Result<(), Box<dyn Error>> {
    let _etherscan_key = env::var("ETHERSCAN_API_KEY").unwrap();
    let etherscan_client = Client::new(Chain::Mainnet, _etherscan_key).unwrap();

    let mut address_book = HashMap::new();

    address_book.insert("UNISWAP_V3_FACTORY", UNISWAP_V3_FACTORY);

    let mut parsed_addr;
    for (name, addr) in address_book {
        parsed_addr = addr.parse::<H160>()?;
        generate_abigen(&etherscan_client, name, parsed_addr).await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::abigen::generate_abigen_for_addresses;
    use anyhow::Result;

    #[tokio::test]
    async fn test_generate_abigen_for_address() -> Result<()> {
        generate_abigen_for_addresses().await.unwrap();

        Ok(())
    }
}
