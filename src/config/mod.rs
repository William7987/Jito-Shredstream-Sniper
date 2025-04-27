use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use std::env;
use dotenvy::dotenv;

#[derive(Clone)]
pub struct Config {
    pub server_url: String,
    pub token_creator_pubkey: Pubkey,
}

impl Config {
    pub fn new() -> Self {
        // 加载环境变量
        dotenv().ok();
        
        // 从环境变量获取服务器URL，如果未设置则程序停止
        let server_url = env::var("SERVER_URL").expect("环境变量SERVER_URL未设置");
        
        Self {
            server_url,
            token_creator_pubkey: Pubkey::from_str("TSLvdd1pWpHVjahSpsvCXUbgwsL3JAcvokwaKt1eokM").unwrap(),
        }
    }
} 