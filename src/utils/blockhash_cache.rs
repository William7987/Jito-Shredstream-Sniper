use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::{commitment_config::{CommitmentConfig, CommitmentLevel}, hash::Hash};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// 区块哈希缓存，用于减少RPC调用
pub struct BlockhashCache {
    rpc_client: RpcClient,
    cached_blockhash: Arc<Mutex<Option<(Hash, Instant)>>>,
    max_age: Duration,
}

impl BlockhashCache {
    /// 创建一个新的区块哈希缓存
    /// 
    /// # 参数
    /// 
    /// * `rpc_url` - RPC节点URL
    /// * `max_age_ms` - 缓存的最大有效期（毫秒）
    pub fn new(rpc_url: &str, max_age_ms: u64) -> Self {
        Self {
            rpc_client: RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed()),
            cached_blockhash: Arc::new(Mutex::new(None)),
            max_age: Duration::from_millis(max_age_ms),
        }
    }
    
    /// 获取最新的区块哈希，如果缓存有效则从缓存中获取
    pub async fn get_latest_blockhash(&self) -> Result<Hash, Box<dyn std::error::Error + Send + Sync>> {
        let mut cache = self.cached_blockhash.lock().await;
        
        // 检查缓存是否有效
        if let Some((hash, timestamp)) = &*cache {
            if timestamp.elapsed() < self.max_age {
                println!("使用缓存的区块哈希");
                return Ok(*hash);
            }
        }
        
        // 缓存不存在或已过期，从RPC获取
        println!("获取新的区块哈希");
        let blockhash = self.rpc_client
            .get_latest_blockhash_with_commitment(CommitmentConfig {
                commitment: CommitmentLevel::Confirmed,
            })
            .await?
            .0;
        
        // 更新缓存
        *cache = Some((blockhash, Instant::now()));
        
        Ok(blockhash)
    }
} 