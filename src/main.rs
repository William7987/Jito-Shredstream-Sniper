mod config;
mod client;
mod processor;
mod utils;
mod instruction;
mod transaction;

use config::Config;
use client::ShredstreamClient;
use processor::TransactionProcessor;
use utils::deserialize_entries;
use utils::redis::RedisClient;
use utils::auto_trader::AutoTrader;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::Mutex;
use std::env;
use dotenvy::dotenv;

#[tokio::main]
async fn main() {
    // 加载环境变量
    dotenv().ok();
    
    // 获取配置
    let config = Config::new();
    let client_result = ShredstreamClient::new(config.clone()).await;
    let mut client = match client_result {
        Ok(client) => client,
        Err(e) => {
            println!("创建客户端失败: {:?}", e);
            return;
        }
    };
    
    let mut processor = TransactionProcessor::new(config.token_creator_pubkey);
    
    // 获取Redis配置
    let redis_url = env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    println!("连接Redis: {}", redis_url);
    
    // 获取RPC和私钥
    let rpc_url = env::var("RPC_URL").unwrap_or_else(|_| "https://api.mainnet-beta.solana.com".to_string());
    let private_key = match env::var("PRIVATE_KEY") {
        Ok(key) => key,
        Err(_) => {
            println!("需要设置PRIVATE_KEY环境变量");
            return;
        }
    };
    
    // 初始化Redis客户端
    let redis_client_result = RedisClient::new(&redis_url).await;
    let redis_client = match redis_client_result {
        Ok(client) => {
            println!("Redis连接成功");
            Arc::new(client)
        },
        Err(e) => {
            println!("Redis连接失败: {:?}，将不使用自动交易功能", e);
            return;
        }
    };
    
    // 初始化自动交易器
    let auto_trader = AutoTrader::new(
        redis_client.clone(),
        rpc_url.clone(),
        private_key.clone()
    ).await;
    
    // 从环境变量读取狙击价格范围
    let min_sol_str = env::var("MIN_SOL_PRICE").unwrap_or_else(|_| "0.5".to_string());
    let max_sol_str = env::var("MAX_SOL_PRICE").unwrap_or_else(|_| "3.0".to_string());
    let buy_sol_str = env::var("BUY_SOL_AMOUNT").unwrap_or_else(|_| "0.1".to_string());
    let sell_delay_ms = env::var("SELL_DELAY_MS").unwrap_or_else(|_| "5000".to_string());
    
    // 将SOL单位的浮点数转换为lamports整数
    let min_sol = (min_sol_str.parse::<f64>().unwrap_or(0.5) * 1_000_000_000.0) as u64;
    let max_sol = (max_sol_str.parse::<f64>().unwrap_or(3.0) * 1_000_000_000.0) as u64;
    let buy_sol = (buy_sol_str.parse::<f64>().unwrap_or(0.1) * 1_000_000_000.0) as u64;
    let sell_delay = sell_delay_ms.parse::<u64>().unwrap_or(5000);
    
    // 创建自动交易器的互斥锁
    let auto_trader = Arc::new(Mutex::new(auto_trader));
    
    // 设置交易器参数和启动
    {
        let mut trader = auto_trader.lock().await;
        trader.set_price_range(min_sol, max_sol).await;
        trader.set_buy_amount(buy_sol).await;
        trader.set_sell_delay(sell_delay).await;
        trader.start();
    }
    
    // 为处理器设置自动交易器
    processor.set_auto_trader(Arc::clone(&auto_trader));
    
    println!("开始监听Jito Shredstream数据...");
    println!("将自动狙击价格在 {} - {} SOL 的新代币", min_sol_str, max_sol_str);
    println!("每次将投入 {} SOL 进行购买", buy_sol_str);
    println!("并在 {}ms 后自动卖出", sell_delay);
    println!("---------------------------");

    // 主循环 - 持续监听Shredstream数据
    loop {
        match client.subscribe_entries().await {
            Ok(mut stream) => {
                let process_result = async {
                    while let Some(entry) = match stream.message().await {
                        Ok(entry) => entry,
                        Err(e) => {
                            println!("获取消息失败: {:?}", e);
                            return Ok(());
                        }
                    } {
                        match deserialize_entries(&entry.entries) {
                            Ok(entries) => {
                                if let Err(e) = processor.process_entries(entries, entry.slot) {
                                    println!("处理条目失败: {:?}", e);
                                }
                            },
                            Err(e) => {
                                println!("反序列化失败: {e}");
                            }
                        }
                    }
                    Ok::<(), ()>(())
                }.await;
                
                if let Err(_) = process_result {
                    println!("处理消息循环发生致命错误");
                }
            }
            Err(e) => {
                println!("连接断开: {e}");
                println!("5秒后重新连接...");
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }
}
