use chrono::Local;
use solana_sdk::{message::VersionedMessage, pubkey::Pubkey, transaction::VersionedTransaction};
use solana_entry::entry::Entry;
use crate::instruction::parse_instruction_data;
use std::error::Error;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use crate::utils::auto_trader::AutoTrader;

// 用于存储代币的虚拟储备信息
struct TokenReserves {
    virtual_sol_reserves: u64,    // 虚拟SOL储备
    virtual_token_reserves: u64,  // 虚拟代币储备
}

pub struct TransactionProcessor {
    token_creator_pubkey: Pubkey,
    // 使用HashMap跟踪各个代币的虚拟储备状态
    token_reserves: HashMap<String, TokenReserves>,
    // 自动交易器
    auto_trader: Option<Arc<Mutex<AutoTrader>>>,
}

impl TransactionProcessor {
    pub fn new(token_creator_pubkey: Pubkey) -> Self {
        Self { 
            token_creator_pubkey,
            token_reserves: HashMap::new(),
            auto_trader: None,
        }
    }
    
    // 设置自动交易器
    pub fn set_auto_trader(&mut self, auto_trader: Arc<Mutex<AutoTrader>>) {
        self.auto_trader = Some(auto_trader);
        println!("已设置自动交易器");
    }

    pub fn process_entries(&mut self, entries: Vec<Entry>, slot: u64) -> Result<(), Box<dyn Error>> {
        for entry in entries {
            for tx_data in entry.transactions {
                let transaction = tx_data;
                
                match &transaction.message {
                    VersionedMessage::V0(message) => self.process_message_v0(message, &transaction, slot)?,
                    VersionedMessage::Legacy(message) => self.process_message_legacy(message, &transaction, slot)?,
                }
            }
        }
        Ok(())
    }

    fn process_message_v0(&mut self, message: &solana_sdk::message::v0::Message, transaction: &VersionedTransaction, slot: u64) -> Result<(), Box<dyn Error>> {
        if message.account_keys.contains(&self.token_creator_pubkey) {
            println!("\n{}", "-".repeat(80));
            println!("[{}] Pumpfun内盘创建代币事件:", Local::now().format("%Y-%m-%d %H:%M:%S%.3f"));
            println!("Slot: {}", slot);
            println!("Signatures: {}", transaction.signatures[0]);
            
            // 提取关键账户地址
            let mint_address = message.account_keys[1].to_string();
            let bonding_curve = message.account_keys[2].to_string();
            
            println!("Mint: {}", mint_address);
            println!("Bonding_Curve: {}", bonding_curve);

            // 检查交易中的所有指令
            for instruction in &message.instructions {
                let program_id = message.account_keys[instruction.program_id_index as usize].to_string();
                
                // 如果指令是针对目标程序的
                if program_id == self.token_creator_pubkey.to_string() || program_id == "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P" {
                    // 尝试解析指令
                    if let Ok((instruction_type, create_event, buy_event)) = parse_instruction_data(&instruction.data) {
                        match instruction_type.as_str() {
                            "CreateEvent" => {
                                if let Some(event) = create_event {
                                    println!("Token_Metadata:");
                                    println!("  Name: {}", event.name);
                                    println!("  Symbol: {}", event.symbol);
                                    println!("  URI: {}", event.uri);
                                    println!("  Creator: {}", event.user);
                                    
                                    // 为新代币初始化虚拟储备
                                    if !self.token_reserves.contains_key(&mint_address) {
                                        // 初始化虚拟储备值 - 根据交易记录调整为更准确的值
                                        let virtual_sol_reserves = 30_000_000_000;             // 30 SOL (lamports)
                                        let virtual_token_reserves = 1_073_000_000_000_000;    // 约10.73亿代币（精度为6）
                                        
                                        self.token_reserves.insert(mint_address.clone(), TokenReserves {
                                            virtual_sol_reserves,
                                            virtual_token_reserves,
                                        });
                                    }
                                }
                            }
                            "Buy" => {
                                if let Some(event) = buy_event {
                                    // 直接使用原始值，保留精度
                                    let token_amount = event.amount;
                                    let sol_amount = event.max_sol_cost;
                                    
                                    // 简化显示的打印输出
                                    let token_amount_display = token_amount as f64 / 1_000_000.0; // 考虑6位小数精度
                                    let sol_amount_display = sol_amount as f64 / 1_000_000_000.0;
                                    
                                    println!("Buy_Event:");
                                    println!("  User: {}", message.account_keys[0]);
                                    println!("  SOL_Amount: {:.6}", sol_amount_display);
                                    println!("  Token_Amount: {:.6}", token_amount_display);
                                    
                                    // 检查是否满足狙击条件
                                    if let Some(auto_trader) = &self.auto_trader {
                                        // 使用克隆的mint_address和auto_trader，以便在异步闭包中使用
                                        let mint = mint_address.clone();
                                        let trader_clone = Arc::clone(auto_trader);
                                        
                                        // 使用tokio::spawn启动异步任务，检查是否需要狙击
                                        let sol_amount_copy = sol_amount;
                                        let sol_display = sol_amount_display;
                                        
                                        // 获取当前代币价格
                                        let token_price = if let Some(reserves) = self.token_reserves.get(&mint_address) {
                                            let virtual_sol = reserves.virtual_sol_reserves as f64 / 1_000_000_000.0;
                                            let virtual_token = reserves.virtual_token_reserves as f64 / 1_000_000.0;
                                            virtual_sol / virtual_token
                                        } else {
                                            0.000000033 // 默认估计值，如果无法获取实际价格
                                        };
                                        
                                        // 传递slot，以便用于获取合适的区块哈希
                                        let current_slot = slot;
                                        
                                        // 使用tokio::spawn来执行异步代码
                                        tokio::spawn(async move {
                                            // 记录开始检查的时间，用于监控处理延迟
                                            let start_time = std::time::Instant::now();
                                            
                                            let should_snipe = {
                                                let trader = trader_clone.lock().await;
                                                trader.should_snipe(sol_amount_copy)
                                            };
                                            
                                            if should_snipe {
                                                println!("检测到符合条件的购买，准备狙击: {} SOL", sol_display);
                                                println!("使用slot: {}, 当前时间: {}", current_slot, Local::now().format("%H:%M:%S%.3f"));
                                                println!("从检测到需要狙击到准备狙击的延迟: {:.3}ms", start_time.elapsed().as_millis());
                                                
                                                // 获取锁以执行狙击，传递slot
                                                let trader = trader_clone.lock().await;
                                                if let Err(e) = trader.snipe_token(&mint, token_price, Some(current_slot)).await {
                                                    println!("狙击失败: {:?}", e);
                                                }
                                            }
                                        });
                                    }
                                    
                                    // 更新虚拟储备（仅用于内部计算，不作为真实值显示）
                                    if let Some(reserves) = self.token_reserves.get_mut(&mint_address) {
                                        // 更新前的状态
                                        let old_virtual_token = reserves.virtual_token_reserves;
                                        
                                        // 更新虚拟储备，添加溢出检查
                                        reserves.virtual_sol_reserves = reserves.virtual_sol_reserves.saturating_add(sol_amount);
                                        
                                        // 使用saturating_sub避免溢出
                                        if token_amount <= reserves.virtual_token_reserves {
                                            reserves.virtual_token_reserves = reserves.virtual_token_reserves.saturating_sub(token_amount);
                                        }
                                        
                                        // 计算价格（使用虚拟储备）
                                        let virtual_sol = reserves.virtual_sol_reserves as f64 / 1_000_000_000.0;
                                        let virtual_token = reserves.virtual_token_reserves as f64 / 1_000_000.0;
                                        let price = virtual_sol / virtual_token;
                                        
                                        // realSolReserves和realTokenReserves实际上只是从交易中提取的数据，而不是真实的储备状态
                                        // realSolReserves通常就是交易中投入的SOL
                                        let real_sol_reserves = sol_amount_display;
                                        
                                        // realTokenReserves是基于交易前的代币储备减去获得的代币数量
                                        // 使用checked_sub避免溢出，如果溢出就使用0
                                        let real_token_reserves = if old_virtual_token >= token_amount {
                                            (old_virtual_token - token_amount) as f64 / 1_000_000.0
                                        } else {
                                            0.0 // 如果发生溢出，则显示0
                                        };
                                        
                                        println!("  realSolReserves: {:.6}", real_sol_reserves);
                                        println!("  realTokenReserves: {:.6}", real_token_reserves);
                                        println!("  Price: {:.9}", price);
                                    }
                                }
                            }
                            _ => {
                                // 其他指令类型暂不处理
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn process_message_legacy(&mut self, message: &solana_sdk::message::Message, transaction: &VersionedTransaction, slot: u64) -> Result<(), Box<dyn Error>> {
        if message.account_keys.contains(&self.token_creator_pubkey) {
            println!("\n{}", "-".repeat(80));
            println!("[{}] Pumpfun内盘创建代币事件:", Local::now().format("%Y-%m-%d %H:%M:%S%.3f"));
            println!("Slot: {}", slot);
            println!("Signatures: {}", transaction.signatures[0]);
            
            // 提取关键账户地址
            let mint_address = message.account_keys[1].to_string();
            let bonding_curve = message.account_keys[2].to_string();
            
            println!("Mint: {}", mint_address);
            println!("Bonding_Curve: {}", bonding_curve);

            // 检查交易中的所有指令
            for instruction in &message.instructions {
                let program_id = message.account_keys[instruction.program_id_index as usize].to_string();
                
                // 如果指令是针对目标程序的
                if program_id == self.token_creator_pubkey.to_string() || program_id == "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P" {
                    // 尝试解析指令
                    if let Ok((instruction_type, create_event, buy_event)) = parse_instruction_data(&instruction.data) {
                        match instruction_type.as_str() {
                            "CreateEvent" => {
                                if let Some(event) = create_event {
                                    println!("Token_Metadata:");
                                    println!("  Name: {}", event.name);
                                    println!("  Symbol: {}", event.symbol);
                                    println!("  URI: {}", event.uri);
                                    println!("  Creator: {}", event.user);
                                    
                                    // 为新代币初始化虚拟储备
                                    if !self.token_reserves.contains_key(&mint_address) {
                                        // 初始化虚拟储备值 - 根据交易记录调整为更准确的值
                                        let virtual_sol_reserves = 30_000_000_000;             // 30 SOL (lamports)
                                        let virtual_token_reserves = 1_073_000_000_000_000;    // 约10.73亿代币（精度为6）
                                        
                                        self.token_reserves.insert(mint_address.clone(), TokenReserves {
                                            virtual_sol_reserves,
                                            virtual_token_reserves,
                                        });
                                    }
                                }
                            }
                            "Buy" => {
                                if let Some(event) = buy_event {
                                    // 直接使用原始值，保留精度
                                    let token_amount = event.amount;
                                    let sol_amount = event.max_sol_cost;
                                    
                                    // 简化显示的打印输出
                                    let token_amount_display = token_amount as f64 / 1_000_000.0; // 考虑6位小数精度
                                    let sol_amount_display = sol_amount as f64 / 1_000_000_000.0;
                                    
                                    println!("Buy_Event:");
                                    println!("  User: {}", message.account_keys[0]);
                                    println!("  SOL_Amount: {:.6} SOL", sol_amount_display);
                                    println!("  Token_Amount: {:.6} ", token_amount_display);
                                    
                                    // 检查是否满足狙击条件
                                    if let Some(auto_trader) = &self.auto_trader {
                                        // 使用克隆的mint_address和auto_trader，以便在异步闭包中使用
                                        let mint = mint_address.clone();
                                        let trader_clone = Arc::clone(auto_trader);
                                        
                                        // 使用tokio::spawn启动异步任务，检查是否需要狙击
                                        let sol_amount_copy = sol_amount;
                                        let sol_display = sol_amount_display;
                                        
                                        // 获取当前代币价格
                                        let token_price = if let Some(reserves) = self.token_reserves.get(&mint_address) {
                                            let virtual_sol = reserves.virtual_sol_reserves as f64 / 1_000_000_000.0;
                                            let virtual_token = reserves.virtual_token_reserves as f64 / 1_000_000.0;
                                            virtual_sol / virtual_token
                                        } else {
                                            0.000000033 // 默认估计值，如果无法获取实际价格
                                        };
                                        
                                        // 传递slot，以便用于获取合适的区块哈希
                                        let current_slot = slot;
                                        
                                        // 使用tokio::spawn来执行异步代码
                                        tokio::spawn(async move {
                                            // 记录开始检查的时间，用于监控处理延迟
                                            let start_time = std::time::Instant::now();
                                            
                                            let should_snipe = {
                                                let trader = trader_clone.lock().await;
                                                trader.should_snipe(sol_amount_copy)
                                            };
                                            
                                            if should_snipe {
                                                println!("检测到符合条件的购买，准备狙击: {} SOL", sol_display);
                                                println!("使用slot: {}, 当前时间: {}", current_slot, Local::now().format("%H:%M:%S%.3f"));
                                                println!("从检测到需要狙击到准备狙击的延迟: {:.3}ms", start_time.elapsed().as_millis());
                                                
                                                // 获取锁以执行狙击，传递slot
                                                let trader = trader_clone.lock().await;
                                                if let Err(e) = trader.snipe_token(&mint, token_price, Some(current_slot)).await {
                                                    println!("狙击失败: {:?}", e);
                                                }
                                            }
                                        });
                                    }
                                    
                                    // 更新虚拟储备（仅用于内部计算，不作为真实值显示）
                                    if let Some(reserves) = self.token_reserves.get_mut(&mint_address) {
                                        // 更新前的状态
                                        let old_virtual_token = reserves.virtual_token_reserves;
                                        
                                        // 更新虚拟储备，添加溢出检查
                                        reserves.virtual_sol_reserves = reserves.virtual_sol_reserves.saturating_add(sol_amount);
                                        
                                        // 使用saturating_sub避免溢出
                                        if token_amount <= reserves.virtual_token_reserves {
                                            reserves.virtual_token_reserves = reserves.virtual_token_reserves.saturating_sub(token_amount);
                                        }
                                        
                                        // 计算价格（使用虚拟储备）
                                        let virtual_sol = reserves.virtual_sol_reserves as f64 / 1_000_000_000.0;
                                        let virtual_token = reserves.virtual_token_reserves as f64 / 1_000_000.0;
                                        let price = virtual_sol / virtual_token;
                                        
                                        // realSolReserves和realTokenReserves实际上只是从交易中提取的数据，而不是真实的储备状态
                                        // realSolReserves通常就是交易中投入的SOL
                                        let real_sol_reserves = sol_amount_display;
                                        
                                        // realTokenReserves是基于交易前的代币储备减去获得的代币数量
                                        // 使用checked_sub避免溢出，如果溢出就使用0
                                        let real_token_reserves = if old_virtual_token >= token_amount {
                                            (old_virtual_token - token_amount) as f64 / 1_000_000.0
                                        } else {
                                            0.0 // 如果发生溢出，则显示0
                                        };
                                        
                                        println!("  realSolReserves: {:.6}", real_sol_reserves);
                                        println!("  realTokenReserves: {:.6}", real_token_reserves);
                                        println!("  Price: {:.9}", price);
                                    }
                                }
                            }
                            _ => {
                                // 其他指令类型暂不处理
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
} 