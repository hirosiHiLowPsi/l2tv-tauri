use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum L2tvError {
    #[error("{0}")]
    Message(String),
    #[error("ファイル操作に失敗しました: {0}")]
    Io(#[from] std::io::Error),
    #[error("データベースの読み込みに失敗しました: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("通信に失敗しました: {0}")]
    Http(#[from] reqwest::Error),
    #[error("JSONの処理に失敗しました: {0}")]
    Json(#[from] serde_json::Error),
    #[error("URLの形式が正しくありません: {0}")]
    Url(#[from] url::ParseError),
}

impl Serialize for L2tvError {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, L2tvError>;

pub fn message(value: impl Into<String>) -> L2tvError {
    L2tvError::Message(value.into())
}
