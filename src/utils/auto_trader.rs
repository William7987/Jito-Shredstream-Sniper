use std::error::Error;
use std::sync::Arc;
use tokio::time::{sleep, Duration};
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use tokio::task::JoinHandle;
use crate::utils::redis::RedisClient;
use crate::transaction::{pump_buy, pump_sell};
use crate::utils::blockhash_cache::BlockhashCache;
use redis::RedisError;

pub struct AutoTrader {
    redis_client: Arc<RedisClient>,
    rpc_url: String,
    private_key: String,
    running: bool,
    min_sol_price: u64,
    max_sol_price: u64,
    buy_amount: u64,     // 买入金额 (lamports)
    sell_delay_ms: u64,  // 卖出延迟时间 (毫秒)
    blockhash_cache: Arc<BlockhashCache>, // 添加区块哈希缓存
}

impl AutoTrader {
    // 创建新的自动交易器，现在需要异步初始化
    pub async fn new(
        redis_client: Arc<RedisClient>,
        rpc_url: String,
        private_key: String,
    ) -> Self {
        // 默认设置
        let min_sol_price = 500_000_000; // 0.5 SOL
        let max_sol_price = 1_000_000_000; // 1 SOL
        let buy_amount = 100_000_000; // 0.1 SOL
        let sell_delay_ms = 5000; // 5秒后自动卖出
        
        // 创建区块哈希缓存，缓存时间减小到500毫秒，以保持区块哈希更新但又不频繁请求
        let blockhash_cache = Arc::new(BlockhashCache::new(&rpc_url, 500));
        
        Self {
            redis_client,
            rpc_url,
            private_key,
            running: false,
            min_sol_price,
            max_sol_price,
            buy_amount,
            sell_delay_ms,
            blockhash_cache,
        }
    }
    
    // 设置价格范围
    pub async fn set_price_range(&mut self, min_sol_price: u64, max_sol_price: u64) {
        self.min_sol_price = min_sol_price;
        self.max_sol_price = max_sol_price;
        println!("设置狙击价格范围: {} - {} SOL", 
                 min_sol_price as f64 / 1_000_000_000.0,
                 max_sol_price as f64 / 1_000_000_000.0);
    }
    
    // 设置买入金额
    pub async fn set_buy_amount(&mut self, buy_amount: u64) {
        self.buy_amount = buy_amount;
        println!("设置狙击购买金额: {} SOL", buy_amount as f64 / 1_000_000_000.0);
    }
    
    // 设置卖出延迟时间
    pub async fn set_sell_delay(&mut self, sell_delay_ms: u64) {
        self.sell_delay_ms = sell_delay_ms;
        println!("设置自动卖出延迟: {}ms", sell_delay_ms);
    }
    
    // 启动自动交易后台任务
    pub fn start(&mut self) -> JoinHandle<Result<(), Box<dyn Error + Send + Sync>>> {
        self.running = true;
        let rpc_url = self.rpc_url.clone();
        let private_key = self.private_key.clone();
        let redis_client = self.redis_client.clone();
        let blockhash_cache = self.blockhash_cache.clone(); // 克隆缓存引用
        
        println!("启动自动交易后台任务");
        
        // 创建后台任务处理自动卖出逻辑
        tokio::spawn(async move {
            // 自动卖出检查任务
            let sell_task = tokio::spawn({
                let redis_client = redis_client.clone();
                let rpc_url = rpc_url.clone();
                let private_key = private_key.clone();
                let blockhash_cache = blockhash_cache.clone(); // 为内部任务克隆缓存引用
                
                async move {
                    println!("启动自动卖出检查");
                    
                    loop {
                        // 获取并移除所有需要卖出的代币 - 异步版本
                        match redis_client.get_and_remove_mints_to_sell().await {
                            Ok(mints) => {
                                if !mints.is_empty() {
                                    // 如果有代币要卖出，预先获取一次区块哈希
                                    // 这样可以减少每个交易单独获取哈希的次数
                                    let blockhash = match blockhash_cache.get_latest_blockhash().await {
                                        Ok(hash) => Some(hash),
                                        Err(e) => {
                                            println!("获取区块哈希失败: {:?}", e);
                                            None
                                        }
                                    };

                                    for mint in mints {
                                        // 执行自动卖出操作
                                        match Pubkey::from_str(&mint) {
                                            Ok(mint_pubkey) => {
                                                println!("执行自动卖出: {}", mint);
                                                
                                                // 获取之前存储的代币数量
                                                match redis_client.get_mint_amount(&mint).await {
                                                    Ok(Some(token_amount)) => {
                                                        println!("尝试卖出: {} 代币", token_amount);
                                                        
                                                        if let Err(e) = pump_sell(
                                                            &rpc_url,
                                                            &private_key,
                                                            mint_pubkey,
                                                            token_amount, // 使用存储的代币数量
                                                            0, // 最低接收0 SOL
                                                            None, // 不使用特定的slot
                                                            blockhash.clone() // 使用缓存的区块哈希
                                                        ).await {
                                                            println!("自动卖出失败: {:?}", e);
                                                        }
                                                    },
                                                    Ok(None) => {
                                                        // 如果找不到存储的数量，使用估算的数量
                                                        // 这种情况应该很少发生，因为我们在买入时会存储数量
                                                        let buy_sol = 100_000_000; // 0.1 SOL in lamports
                                                        
                                                        // 使用默认价格估计值
                                                        let default_price = 0.000000033;
                                                        
                                                        // 将SOL转换为实际单位
                                                        let buy_sol_f64 = buy_sol as f64 / 1_000_000_000.0;
                                                        
                                                        // 计算不含精度的代币数量
                                                        let token_amount_no_precision = buy_sol_f64 / default_price;
                                                        
                                                        // 减少15%的数量以避免滑点错误
                                                        let reduced_amount = token_amount_no_precision * 0.85;
                                                        
                                                        // 精度因子为10^6
                                                        let precision_factor = 1_000_000.0;
                                                        
                                                        // 计算含精度的代币数量，向下取整
                                                        let token_amount = (reduced_amount * precision_factor).floor() as u64;
                                                        
                                                        println!("未找到存储的代币数量，使用估算值: {} 代币(含精度)", token_amount);
                                                        
                                                        if let Err(e) = pump_sell(
                                                            &rpc_url,
                                                            &private_key,
                                                            mint_pubkey,
                                                            token_amount,
                                                            0, // 最低接收0 SOL
                                                            None, // 不使用特定的slot
                                                            blockhash.clone() // 使用缓存的区块哈希
                                                        ).await {
                                                            println!("自动卖出失败: {:?}", e);
                                                        }
                                                    },
                                                    Err(e) => {
                                                        println!("获取代币数量失败: {:?}", e);
                                                    }
                                                }
                                            },
                                            Err(e) => {
                                                println!("代币地址无效: {} - {:?}", mint, e);
                                            }
                                        }
                                    }
                                }
                            },
                            Err(e) => println!("获取要卖出的代币失败: {:?}", e)
                        }
                        
                        // 每秒检查一次
                        sleep(Duration::from_secs(1)).await;
                    }
                }
            });
            
            // 等待卖出任务完成（理论上不会完成，除非出错）
            if let Err(e) = sell_task.await {
                println!("自动卖出任务异常终止: {:?}", e);
            }
            
            Ok(())
        })
    }
    
    // 狙击指定代币
    pub async fn snipe_token(&self, token_mint: &str, token_price: f64, slot: Option<u64>) -> Result<(), Box<dyn Error>> {
        // 将代币地址转为Pubkey
        let mint_pubkey = Pubkey::from_str(token_mint)?;
        
        // 使用配置的买入金额
        let buy_sol = self.buy_amount;
        
        // 将buy_sol转换为SOL单位(从lamports)
        let buy_sol_f64 = buy_sol as f64 / 1_000_000_000.0;
        
        // 确保价格不为零，避免除零错误
        if token_price <= 0.0 {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other, 
                format!("无效的代币价格: {}", token_price)
            )));
        }
        
        // 计算不含精度的代币数量
        let token_amount_no_precision = buy_sol_f64 / token_price;
        
        // 精度因子为10^6
        let precision_factor = 1_000_000.0;
        
        // 计算含精度的代币数量，向下取整
        // 减少15%的购买数量，以避免滑点错误
        let reduced_amount = token_amount_no_precision * 0.85; 
        let token_amount = (reduced_amount * precision_factor).floor() as u64;
        
        // 记录开始狙击的时间戳
        let start_time = std::time::Instant::now();
        
        println!("开始狙击代币 {} (slot: {:?})", token_mint, slot);
        println!("投入: {} SOL", buy_sol_f64);
        println!("实际价格: {} SOL/token", token_price);
        println!("计算得出代币数量: {:.2} (无精度)", token_amount_no_precision);
        println!("减少后的数量: {:.2} (无精度)", reduced_amount);
        println!("尝试购买: {} 代币(含精度)", token_amount);
        
        // 获取缓存的区块哈希，快速路径优先
        let blockhash = match self.blockhash_cache.get_latest_blockhash().await {
            Ok(hash) => Some(hash),
            Err(e) => {
                println!("获取区块哈希失败: {:?}", e);
                None
            }
        };
        
        // 买入代币，使用缓存的区块哈希
        match pump_buy(
            &self.rpc_url,
            &self.private_key,
            mint_pubkey,
            token_amount,
            buy_sol,
            slot,
            blockhash
        ).await {
            Ok(signature) => {
                let elapsed = start_time.elapsed();
                println!("狙击成功! 交易签名: {}", signature);
                println!("狙击总耗时: {:.3}ms", elapsed.as_millis());
                
                // 买入成功后，存储代币地址和购买数量到Redis，设置延迟后自动卖出
                self.redis_client.store_mint_with_amount(token_mint, token_amount, self.sell_delay_ms).await?;
                
                Ok(())
            },
            Err(e) => {
                let elapsed = start_time.elapsed();
                println!("狙击失败: {:?}", e);
                println!("失败耗时: {:.3}ms", elapsed.as_millis());
                Err(Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("狙击失败: {:?}", e))))
            }
        }
    }
    
    // 判断是否应该狙击
    pub fn should_snipe(&self, sol_amount: u64) -> bool {
        sol_amount >= self.min_sol_price && sol_amount <= self.max_sol_price
    }
}