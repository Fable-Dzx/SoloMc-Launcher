//! 统一错误类型

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum LauncherError {
    #[error("网络错误: {0}")]
    Reqwest(#[from] reqwest::Error),
    #[error("JSON 解析错误: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IO 错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("Zip 解压错误: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("HTTP 状态码异常: {0}")]
    HttpStatus(reqwest::StatusCode),
    #[error("{0}")]
    Msg(String),
}

impl LauncherError {
    pub fn msg<T: Into<String>>(m: T) -> Self {
        LauncherError::Msg(m.into())
    }
}

impl Serialize for LauncherError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, LauncherError>;
