use redis::{AsyncCommands, Client, RedisError, aio::Connection as AsyncConnection};
use tokio::sync::Mutex;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct RedisClient {
    client: Client,
    connection: Arc<Mutex<AsyncConnection>>,
}

impl RedisClient {
    pub async fn new(redis_url: &str) -> Result<Self, RedisError> {
        let client = Client::open(redis_url)?;
        let connection = Arc::new(Mutex::new(client.get_async_connection().await?));
        
        Ok(Self {
            client,
            connection,
        })
    }
    
    // 存储Mint地址到Redis，作为自动交易的队列，可指定延迟时间
    pub async fn store_mint_data(&self, mint: &str, delay_ms: u64) -> Result<(), RedisError> {
        let mut conn = self.connection.lock().await;
        
        // 获取当前时间戳作为score，并加上指定延迟时间
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let sell_time = now + delay_ms; // 指定时间后卖出
        
        // 将mint地址添加到有序集合中，score为卖出时间
        conn.zadd("mints_to_sell", mint, sell_time).await?;
        
        println!("已将代币 {} 添加到卖出队列，将在 {}ms 后卖出", mint, delay_ms);
        
        Ok(())
    }
    
    // 存储Mint地址和对应的代币数量，并设置自动卖出时间
    pub async fn store_mint_with_amount(&self, mint: &str, amount: u64, delay_ms: u64) -> Result<(), RedisError> {
        let mut conn = self.connection.lock().await;
        
        // 获取当前时间戳作为score，并加上指定延迟时间
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        let sell_time = now + delay_ms; // 指定时间后卖出
        
        // 将mint地址添加到有序集合中，score为卖出时间
        conn.zadd("mints_to_sell", mint, sell_time).await?;
        
        // 同时保存该代币的数量到另一个哈希表中
        conn.hset("mint_amounts", mint, amount.to_string()).await?;
        
        println!("已将代币 {} (数量: {}) 添加到卖出队列，将在 {}ms 后卖出", mint, amount, delay_ms);
        
        Ok(())
    }
    
    // 获取指定代币的数量
    pub async fn get_mint_amount(&self, mint: &str) -> Result<Option<u64>, RedisError> {
        let mut conn = self.connection.lock().await;
        
        // 从哈希表中获取代币数量
        let amount: Option<String> = conn.hget("mint_amounts", mint).await?;
        
        // 将字符串转换为u64
        match amount {
            Some(amount_str) => {
                match amount_str.parse::<u64>() {
                    Ok(amount) => Ok(Some(amount)),
                    Err(_) => Ok(None)
                }
            },
            None => Ok(None)
        }
    }
    
    // 获取到期需要卖出的代币列表
    pub async fn get_mints_to_sell(&self) -> Result<Vec<String>, RedisError> {
        let mut conn = self.connection.lock().await;
        
        // 获取当前时间戳
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        // 查询score小于等于当前时间的所有mint地址
        let mints_to_sell: Vec<String> = conn.zrangebyscore("mints_to_sell", 0, now).await?;
        
        Ok(mints_to_sell)
    }
    
    // 从Redis中删除已卖出的代币
    pub async fn remove_sold_mint(&self, mint: &str) -> Result<(), RedisError> {
        let mut conn = self.connection.lock().await;
        
        // 从有序集合中删除指定的mint地址
        conn.zrem("mints_to_sell", mint).await?;
        
        // 同时删除代币数量记录
        conn.hdel("mint_amounts", mint).await?;
        
        println!("已从卖出队列中删除代币: {}", mint);
        
        Ok(())
    }
    
    // 获取并删除所有需要卖出的代币
    pub async fn get_and_remove_mints_to_sell(&self) -> Result<Vec<String>, RedisError> {
        // 先获取要卖出的代币
        let mints_to_sell = self.get_mints_to_sell().await?;
        
        if mints_to_sell.is_empty() {
            return Ok(vec![]);
        }
        
        let mut conn = self.connection.lock().await;
        
        // 获取当前时间戳
        let _now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        
        // 删除所有已获取的代币，使用ZREMRANGEBYSCORE命令
        // 注意：redis-rs库中可能没有直接的zremrangebyscore方法，使用zrem代替
        for mint in &mints_to_sell {
            conn.zrem("mints_to_sell", mint).await?;
        }
        
        Ok(mints_to_sell)
    }
}