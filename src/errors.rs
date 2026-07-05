use sqlx::FromRow;
use thiserror::Error;
use std::fmt;

#[derive(Debug, FromRow)]
pub struct Book {
    pub id: i32,
    pub title: String,
    pub author: String,
    pub genre: String,
    pub year: Option<i32>,
    pub read: i32,
    pub rating: Option<i32>,
}

#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Db(#[from] sqlx::Error),

    #[error("Migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("{0}")]
    Message(String),
}

impl fmt::Display for Book {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if self.read != 0 { "[x]" } else { "[ ]" };
        let year_str = self.year.map(|y| y.to_string()).unwrap_or_else(|| "????".to_string());
        write!(f, "{} {:3}. {} by {} ({}) [{}]",
            status, self.id, self.title, self.author, self.genre, year_str)?;
        if let Some(r) = self.rating {
            write!(f, " ★{}", r)?;
        }
        Ok(())
    }
}
