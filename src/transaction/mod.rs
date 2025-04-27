use std::fmt::Error;

use solana_rpc_client::nonblocking::rpc_client::RpcClient;
use solana_rpc_client_api::config::RpcSendTransactionConfig;
use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
    hash::Hash,
    instruction::{AccountMeta, Instruction},
    pubkey::Pubkey,
    signer::Signer,
    system_program,
    transaction::Transaction,
};
use spl_associated_token_account::get_associated_token_address;

// Pump协议相关常量
pub const GLOBAL_ACCOUNT: Pubkey =
    solana_sdk::pubkey!("4wTV1YmiEkRvAtNtsSGPtUrqRYQMe5SKy2uB4Jjaxnjf");
pub const FEE_RECIPIENT: Pubkey =
    solana_sdk::pubkey!("62qc2CNXwrYqQScmEdiZFFAnJR262PxWEuNQtxfafNgV");
pub const EVENT_AUTHORITY: Pubkey = solana_sdk::pubkey!("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1");
pub const PUMP_PROGRAM_ID: Pubkey =
    solana_sdk::pubkey!("6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P");
pub const PROXY_PROGRAM: Pubkey = solana_sdk::pubkey!("AmXoSVCLjsfKrwCUqvkMFXYcDzZ4FeoMYs7SAhGyfMGy");

// 系统账户
pub const SYSVAR_RENT_PUBKEY: Pubkey = solana_sdk::pubkey!("SysvarRent111111111111111111111111111111111");

// 指令鉴别器
pub const PUMP_BUY_SELECTOR: &[u8; 8] = &[82, 225, 119, 231, 78, 29, 45, 70];  // 内盘买入鉴别器
pub const PUMP_SELL_SELECTOR: &[u8; 8] = &[83, 225, 119, 231, 78, 29, 45, 70]; // 内盘卖出鉴别器
pub const ATA_SELECTOR: &[u8; 8] = &[22, 51, 53, 97, 247, 184, 54, 78];        // 创建ATA鉴别器

const BONDING_CURVE_SEED: &[u8] = b"bonding-curve";

/// Pump协议代币买入交易
/// 
/// # 参数
/// 
/// * `rpc_url` - RPC节点URL
/// * `private_key` - 用户私钥
/// * `token_mint` - 代币Mint地址
/// * `token_amount` - 要购买的代币数量
/// * `max_sol_cost` - 最大SOL花费(lamports)
/// * `slot` - 可选的槽号，用于记录日志
/// * `cached_blockhash` - 可选的缓存区块哈希，如果提供则不会查询RPC
pub async fn pump_buy(
    rpc_url: &str, 
    private_key: &str, 
    token_mint: Pubkey, 
    token_amount: u64, 
    max_sol_cost: u64,
    slot: Option<u64>,
    cached_blockhash: Option<Hash>
) -> Result<String, Error> {
    let rpc_client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

    // 构造买入指令数据
    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(PUMP_BUY_SELECTOR);
    data.extend_from_slice(&token_amount.to_le_bytes());
    data.extend_from_slice(&max_sol_cost.to_le_bytes());

    let signer = solana_sdk::signature::Keypair::from_base58_string(private_key);

    // 计算Bonding Curve地址
    let bonding_curve_address =
        Pubkey::find_program_address(&[BONDING_CURVE_SEED, token_mint.as_ref()], &PUMP_PROGRAM_ID);

    // 用户代币关联账户
    let associated_user = get_associated_token_address(&signer.pubkey(), &token_mint);

    // Bonding Curve关联代币账户
    let associated_bonding_curve =
        get_associated_token_address(&bonding_curve_address.0, &token_mint);

    // 构造买入指令
    let buy_instruction = Instruction::new_with_bytes(
        PROXY_PROGRAM,
        &data,
        vec![
            AccountMeta::new_readonly(GLOBAL_ACCOUNT, false),
            AccountMeta::new(FEE_RECIPIENT, false),
            AccountMeta::new_readonly(token_mint, false),
            AccountMeta::new(bonding_curve_address.0, false),
            AccountMeta::new(associated_bonding_curve, false),
            AccountMeta::new(associated_user, false),
            AccountMeta::new(signer.pubkey(), true),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(SYSVAR_RENT_PUBKEY, false), // 修正Rent Sysvar地址
            AccountMeta::new_readonly(EVENT_AUTHORITY, false),
            AccountMeta::new_readonly(PUMP_PROGRAM_ID, false),
        ],
    );

    // 创建ATA指令数据
    let mut ata_data = Vec::with_capacity(9);
    ata_data.extend_from_slice(ATA_SELECTOR);
    ata_data.extend_from_slice(&[0]);

    // 构造创建ATA指令
    let ata_instruction = Instruction::new_with_bytes(
        PROXY_PROGRAM,
        &ata_data,
        vec![
            AccountMeta::new(signer.pubkey(), true),
            AccountMeta::new(associated_user, false),
            AccountMeta::new_readonly(token_mint, false),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(spl_associated_token_account::id(), false),
        ],
    );

    // 添加优先级费用指令 - 提高优先级费用到200000以确保快速处理
    let compute_unit_price_ix = solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(200000);
    
    // 增加最大计算单元以确保交易不会因为计算资源不足而失败
    let compute_unit_limit_ix = solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(200000);

    // 获取区块哈希
    let blockhash = if let Some(hash) = cached_blockhash {
        // 使用提供的缓存区块哈希
        if let Some(slot_num) = slot {
            println!("买入使用相关slot: {} 和缓存区块哈希", slot_num);
        } else {
            println!("买入使用缓存区块哈希");
        }
        hash
    } else {
        // 直接获取最新区块哈希
        if let Some(slot_num) = slot {
            println!("买入使用相关slot: {} 和新获取的区块哈希", slot_num);
        }
        
        rpc_client
            .get_latest_blockhash_with_commitment(CommitmentConfig {
                commitment: CommitmentLevel::Confirmed,
            })
            .await
            .unwrap()
            .0
    };

    // 创建交易
    let transaction = Transaction::new_signed_with_payer(
        &[compute_unit_price_ix, compute_unit_limit_ix, ata_instruction, buy_instruction], // 添加两个优先级指令
        Some(&signer.pubkey()),
        &[&signer],
        blockhash,
    );

    // 发送交易 - 使用最优的交易设置
    match rpc_client
        .send_transaction_with_config(
            &transaction,
            RpcSendTransactionConfig {
                skip_preflight: true,
                preflight_commitment: Some(CommitmentLevel::Processed), // 使用Processed级别以最快返回
                max_retries: Some(0), // 不重试，因为我们需要立即知道结果
                ..Default::default()
            },
        )
        .await
    {
        Ok(signature) => {
            println!("买入交易已提交: {}", signature);
            Ok(signature.to_string())
        },
        Err(e) => {
            println!("买入交易失败: {:?}", e);
            Err(Error)
        }
    }
}

/// Pump协议代币卖出交易
/// 
/// # 参数
/// 
/// * `rpc_url` - RPC节点URL
/// * `private_key` - 用户私钥
/// * `token_mint` - 代币Mint地址
/// * `token_amount` - 要卖出的代币数量
/// * `min_sol_receive` - 最小SOL收益(lamports)
/// * `slot` - 可选的槽号，用于记录日志
/// * `cached_blockhash` - 可选的缓存区块哈希，如果提供则不会查询RPC
pub async fn pump_sell(
    rpc_url: &str, 
    private_key: &str, 
    token_mint: Pubkey, 
    token_amount: u64, 
    min_sol_receive: u64,
    slot: Option<u64>,
    cached_blockhash: Option<Hash>
) -> Result<String, Error> {
    let rpc_client = RpcClient::new_with_commitment(rpc_url.to_string(), CommitmentConfig::confirmed());

    // 构造卖出指令数据
    let mut data = Vec::with_capacity(24);
    data.extend_from_slice(PUMP_SELL_SELECTOR); // 使用内部选择器 PUMPFUN_SELL_SELECTOR
    data.extend_from_slice(&token_amount.to_le_bytes());
    data.extend_from_slice(&min_sol_receive.to_le_bytes());

    let signer = solana_sdk::signature::Keypair::from_base58_string(private_key);

    // 计算Bonding Curve地址
    let bonding_curve_address =
        Pubkey::find_program_address(&[BONDING_CURVE_SEED, token_mint.as_ref()], &PUMP_PROGRAM_ID);

    // 用户代币关联账户
    let associated_user = get_associated_token_address(&signer.pubkey(), &token_mint);

    // Bonding Curve关联代币账户
    let associated_bonding_curve =
        get_associated_token_address(&bonding_curve_address.0, &token_mint);

    // 添加优先级费用指令 - 提高优先级费用到200000以确保快速处理
    let compute_unit_price_ix = solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_price(200000);
    
    // 增加最大计算单元以确保交易不会因为计算资源不足而失败
    let compute_unit_limit_ix = solana_sdk::compute_budget::ComputeBudgetInstruction::set_compute_unit_limit(200000);

    // 构造卖出指令
    let sell_instruction = Instruction::new_with_bytes(
        PROXY_PROGRAM,
        &data,
        vec![
            AccountMeta::new_readonly(GLOBAL_ACCOUNT, false),
            AccountMeta::new(FEE_RECIPIENT, false),
            AccountMeta::new_readonly(token_mint, false),
            AccountMeta::new(bonding_curve_address.0, false),
            AccountMeta::new(associated_bonding_curve, false),
            AccountMeta::new(associated_user, false),
            AccountMeta::new(signer.pubkey(), true),
            AccountMeta::new_readonly(system_program::id(), false),
            AccountMeta::new_readonly(spl_associated_token_account::id(), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(EVENT_AUTHORITY, false),
            AccountMeta::new_readonly(PUMP_PROGRAM_ID, false),
        ],
    );

    // 获取区块哈希
    let blockhash = if let Some(hash) = cached_blockhash {
        // 使用提供的缓存区块哈希
        if let Some(slot_num) = slot {
            println!("卖出使用相关slot: {} 和缓存区块哈希", slot_num);
        } else {
            println!("卖出使用缓存区块哈希");
        }
        hash
    } else {
        // 直接获取最新区块哈希
        if let Some(slot_num) = slot {
            println!("卖出使用相关slot: {} 和新获取的区块哈希", slot_num);
        }
        
        rpc_client
            .get_latest_blockhash_with_commitment(CommitmentConfig {
                commitment: CommitmentLevel::Confirmed,
            })
            .await
            .unwrap()
            .0
    };

    // 创建交易
    let transaction = Transaction::new_signed_with_payer(
        &[compute_unit_price_ix, compute_unit_limit_ix, sell_instruction], // 添加两个优先级指令
        Some(&signer.pubkey()),
        &[&signer],
        blockhash,
    );

    // 发送交易 - 使用最优的交易设置
    match rpc_client
        .send_transaction_with_config(
            &transaction,
            RpcSendTransactionConfig {
                skip_preflight: true,
                preflight_commitment: Some(CommitmentLevel::Processed), // 使用Processed级别以最快返回
                max_retries: Some(0), // 不重试，因为我们需要立即知道结果
                ..Default::default()
            },
        )
        .await
    {
        Ok(signature) => {
            println!("卖出交易已提交: {}", signature);
            Ok(signature.to_string())
        },
        Err(e) => {
            println!("卖出交易失败: {:?}", e);
            Err(Error)
        }
    }
}
