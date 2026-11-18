use serde::Serialize;

pub type Result<T> = std::result::Result<T, CoreError>;

/// Errors carry a stable `code` so the frontend can show a translated message.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("invalid currency code: {0}")]
    InvalidCurrency(String),
    #[error("missing exchange rate for {0}")]
    MissingRate(String),
    #[error("portfolio has no positions")]
    NoPositions,
    #[error("target weights must sum to 100%, got {0}%")]
    WeightsSum(String),
    #[error("invalid value for position {id}: {reason}")]
    InvalidPosition { id: String, reason: String },
    #[error("contribution must not be negative in buy-only mode")]
    NegativeContribution,
    #[error("portfolio value after contribution must be positive")]
    EmptyPortfolio,
    #[error("not an AMI_Calculator export file")]
    NotAnExport,
    #[error("export file version {0} is newer than this app supports")]
    UnsupportedVersion(u32),
    #[error("invalid data: {0}")]
    Validation(String),
    #[error("referenced {table} record {id} does not exist")]
    MissingReference { table: String, id: String },
    #[error("record not found")]
    NotFound,
    #[error("database version {0} is newer than this app supports")]
    DatabaseTooNew(i64),
    #[error("sell on {date} exceeds the quantity held")]
    Oversold { date: String },
    #[error("missing price for {0}")]
    MissingPrice(String),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl CoreError {
    pub fn code(&self) -> &'static str {
        match self {
            CoreError::InvalidCurrency(_) => "invalidCurrency",
            CoreError::MissingRate(_) => "missingRate",
            CoreError::NoPositions => "noPositions",
            CoreError::WeightsSum(_) => "weightsSum",
            CoreError::InvalidPosition { .. } => "invalidPosition",
            CoreError::NegativeContribution => "negativeContribution",
            CoreError::EmptyPortfolio => "emptyPortfolio",
            CoreError::NotAnExport => "notAnExport",
            CoreError::UnsupportedVersion(_) => "unsupportedVersion",
            CoreError::Validation(_) => "validation",
            CoreError::MissingReference { .. } => "missingReference",
            CoreError::NotFound => "notFound",
            CoreError::DatabaseTooNew(_) => "databaseTooNew",
            CoreError::Oversold { .. } => "oversold",
            CoreError::MissingPrice(_) => "missingPrice",
            CoreError::Db(_) => "database",
            CoreError::Json(_) => "json",
            CoreError::Io(_) => "io",
        }
    }
}

/// Shape of an error as sent to the frontend.
#[derive(Debug, Serialize)]
pub struct ErrorPayload {
    pub code: &'static str,
    pub message: String,
}

impl From<CoreError> for ErrorPayload {
    fn from(e: CoreError) -> Self {
        ErrorPayload { code: e.code(), message: e.to_string() }
    }
}
