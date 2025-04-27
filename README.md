# Jito Shredstream Sniper

本项目是基于开源项目Jito Shredstream Client开发的使用Jito Shred为数据源配合Swap合约进行狙击的Demo。通过监听Jito Shredstream数据流，实时分析交易]情况，在符合条件的交易中进行自动狙击操作，以提高获利机会。

## 项目特点

- 实时监听Jito Shredstream数据流
- 快速分析交易小费情况，识别低小费交易
- 自动执行Swap交易进行狙击
- 可配置的交易参数和自动策略
- 内置Redis缓存提高性能

## 设置

1. 确保安装了Rust和Cargo
2. 克隆此仓库并进入项目目录
3. 运行 `cargo build` 编译项目

## 环境变量配置

创建一个`.env`文件在项目根目录，或者通过环境变量设置以下配置：

```bash
# Jito Shred 服务端URL
SERVER_URL=http://127.0.0.1:9999

# Solana RPC节点URL
RPC_URL="https://api.mainnet-beta.solana.com"

# 用户私钥 (Base58格式)
PRIVATE_KEY="your_private_key_here"

# Redis服务器地址
REDIS_URL="redis://127.0.0.1:6379"

# 自动交易配置
MIN_SOL_PRICE="0.5"    # 最小狙击价格 (SOL)
MAX_SOL_PRICE="3.0"    # 最大狙击价格 (SOL)
BUY_SOL_AMOUNT="0.1"   # 每次购买投入金额 (SOL)
SELL_DELAY_MS="5000"   # 卖出延迟时间 (毫秒)
```

## 运行客户端

启动低小费狙击客户端：

```bash
cargo run
```

## 工作原理

1. 客户端连接到Jito Shredstream服务，获取最新的交易数据
2. 分析每个交易的情况，识别低于配置阈值的交易
3. 检测符合条件的交易中的代币创建和Swap操作
4. 根据配置的价格范围和阈值，决定是否进行狙击交易
5. 执行买入操作，并在设定的延迟后自动卖出
6. 使用Redis缓存已处理的交易和相关数据，提高性能

## 配置项说明

- `MIN_SOL_PRICE` 和 `MAX_SOL_PRICE`: 设置狙击交易的价格范围，只会狙击在此范围内的代币
- `BUY_SOL_AMOUNT`: 每次狙击交易投入的SOL金额
- `SELL_DELAY_MS`: 买入成功后自动卖出的延迟时间，可根据市场情况调整

## 注意事项

- 确保您的钱包中有足够的SOL来支付交易
- 狙击交易有风险，可能会因各种原因失败，包括滑点保护、流动性不足等

## 高级用法

### 自定义狙击策略

您可以通过修改`src/utils/auto_trader.rs`文件来自定义狙击策略，调整买入和卖出逻辑。

### 性能优化

- 使用本地的Redis实例可以显著提高性能
- 考虑运行在低延迟的云服务器上，减少网络延迟

## 贡献

欢迎提交问题和Pull请求，一起改进这个项目！ 
