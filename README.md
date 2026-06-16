# PurgeDisk - 企业级 U 盘数据合规销毁工具

符合 **DoD 5220.22-M** 标准的跨平台 U 盘数据彻底销毁软件。

## 特性

- **DoD 5220.22-M 标准**：三轮覆写 — 全 0x00 → 全 0xFF → 密码学安全随机数据
- **完整物理磁盘覆盖**：直接操作原始设备，覆盖所有分区和空闲空间
- **严格设备过滤**：自动排除系统盘和内部硬盘，仅操作可移动 USB 存储
- **加密审计日志**：HMAC-SHA256 签名的防篡改日志，满足企业合规要求
- **跨平台支持**：Windows / macOS / Linux（含麒麟系统）

## 安装

### 从源码编译

```bash
# 安装 Rust (如未安装)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 克隆并编译
git clone <repo-url>
cd PurgeDisk
cargo build --release
```

编译后的二进制文件在 `target/release/purgedisk`。

## 使用方法

### 列出可移动设备

```bash
purgedisk list
```

### 擦写设备

```bash
# 交互式（需要输入序列号确认）
sudo purgedisk wipe --device /dev/rdisk2

# 自动化模式（跳过确认）
sudo purgedisk wipe --device /dev/rdisk2 --yes

# 带验证
sudo purgedisk wipe --device /dev/rdisk2 --verify
```

### 查看审计日志

```bash
# 最近 7 天
purgedisk logs

# 最近 30 天
purgedisk logs --days 30

# 按序列号过滤
purgedisk logs --serial "ABC123"
```

## 覆写算法

| 轮次 | 模式 | 说明 |
|------|------|------|
| 第 1 轮 | `0x00` | 全零填充 |
| 第 2 轮 | `0xFF` | 全一填充 |
| 第 3 轮 | RANDOM | 密码学安全随机数据 |

每轮覆盖整个物理磁盘的每一个字节，包括：
- 所有分区
- 未分配空间
- 文件系统元数据
- 引导扇区

## 审计日志

日志自动保存到 `~/.purgedisk/logs/`，格式为 JSON Lines，每条记录包含：

| 字段 | 说明 |
|------|------|
| `timestamp` | ISO 8601 时间戳 |
| `operator` | 操作人系统用户名 |
| `hostname` | 电脑主机名 |
| `device_serial` | U 盘硬件序列号 |
| `device_size_bytes` | 设备总容量 |
| `algorithm` | 覆写算法 |
| `pass_number` | 当前轮次 |
| `pass_pattern` | 覆写模式 (0x00/0xFF/RANDOM) |
| `status` | 操作状态 |
| `hmac_signature` | HMAC-SHA256 签名 |

## 安全机制

1. **多层设备过滤**：可移动位 + 总线类型 + 系统盘检测
2. **二次确认**：需输入设备序列号才能开始擦写
3. **状态持久化**：每轮开始前写入日志，断电可检测未完成状态
4. **HMAC 签名**：日志条目防篡改，签名密钥基于机器信息生成

## 平台兼容性

| 平台 | 设备发现 | 原始访问 | 状态 |
|------|----------|----------|------|
| macOS | diskutil API | /dev/r* | ✅ 完全支持 |
| Windows | DeviceIoControl | \\\\.\\PhysicalDriveN | ✅ 完全支持 |
| Linux | /sys/block | /dev/sd* | ✅ 完全支持 |
| 麒麟系统 | /sys/block | /dev/sd* | ✅ 兼容（基于 Linux） |

## 权限要求

- **macOS/Linux**: 需要 `sudo` 或 root 权限
- **Windows**: 需要以管理员身份运行

## 许可证

MIT License
