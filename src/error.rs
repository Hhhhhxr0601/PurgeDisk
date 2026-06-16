use thiserror::Error;

/// 应用统一错误类型
#[derive(Error, Debug)]
pub enum PurgeError {
    #[error("设备不存在: {0}")]
    DeviceNotFound(String),

    #[error("权限不足: {0}. 请使用管理员/root权限运行")]
    PermissionDenied(String),

    #[error("目标设备不是可移动存储: {} ({})", path, reason)]
    NotRemovable { path: String, reason: String },

    #[error("目标设备是系统盘，禁止操作: {0}")]
    SystemDiskForbidden(String),

    #[error("设备 I/O 错误: {0}")]
    IoError(#[from] std::io::Error),

    #[error("设备信息获取失败: {0}")]
    DeviceInfoError(String),

    #[error("擦写过程中断: 轮次 {pass}, 已写 {written}/{total} 字节")]
    WipeInterrupted {
        pass: u32,
        written: u64,
        total: u64,
    },

    #[error("日志错误: {0}")]
    LogError(String),

    #[error("加密/签名错误: {0}")]
    CryptoError(String),

    #[error("用户取消操作")]
    UserCancelled,

    #[error("内部错误: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, PurgeError>;
