use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("输入无效：{0}")]
    InvalidInput(String),
    #[error("文件处理失败：{0}")]
    File(String),
    #[error("无法连接 Ollama：{0}")]
    OllamaUnavailable(String),
    #[error("订单识别服务请求失败：{0}")]
    RecognitionService(String),
    #[error("Ollama 中未安装模型：{0}")]
    ModelMissing(String),
    #[error("OCR 返回的数据无效：{0}")]
    InvalidOcr(String),
    #[error("腾讯云 OCR 请求失败：{0}")]
    TencentOcr(String),
    #[error("设置文件处理失败：{0}")]
    Settings(String),
    #[error("数据库操作失败：{0}")]
    Database(String),
    #[error("当前已有识别批次正在运行")]
    BatchBusy,
    #[error("未找到记录：{0}")]
    NotFound(String),
    #[error("记录已确认匹配：{0}，请先在匹配页撤销确认")]
    MatchingLocked(String),
    #[error("导出目标已存在：{0}")]
    DestinationExists(String),
    #[error("内部错误：{0}")]
    Internal(String),
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidInput(_) => "invalid_input",
            Self::File(_) => "file_error",
            Self::OllamaUnavailable(_) => "ollama_unavailable",
            Self::RecognitionService(_) => "recognition_service_error",
            Self::ModelMissing(_) => "model_missing",
            Self::InvalidOcr(_) => "invalid_ocr",
            Self::TencentOcr(_) => "tencent_ocr_error",
            Self::Settings(_) => "settings_error",
            Self::Database(_) => "database_error",
            Self::BatchBusy => "batch_busy",
            Self::NotFound(_) => "not_found",
            Self::MatchingLocked(_) => "matching_locked",
            Self::DestinationExists(_) => "destination_exists",
            Self::Internal(_) => "internal_error",
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: &'static str,
    pub message: String,
}

impl From<AppError> for CommandError {
    fn from(error: AppError) -> Self {
        let code = error.code();
        Self {
            code,
            message: error.to_string(),
        }
    }
}

impl From<sea_orm::DbErr> for AppError {
    fn from(error: sea_orm::DbErr) -> Self {
        Self::Database(error.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        Self::File(error.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
pub type CommandResult<T> = Result<T, CommandError>;
