pub const EXIT_SUCCESS: u8 = 0;
pub const EXIT_FAILURE: u8 = 1;
pub const EXIT_USAGE: u8 = 2;
pub const EXIT_PARTIAL: u8 = 3;
pub const EXIT_CANCELLED: u8 = 130;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CliError {
    pub code: u8,
    pub message: String,
}

impl CliError {
    pub fn usage(message: impl Into<String>) -> Self {
        Self {
            code: EXIT_USAGE,
            message: message.into(),
        }
    }

    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            code: EXIT_FAILURE,
            message: message.into(),
        }
    }

    pub fn cancelled() -> Self {
        Self {
            code: EXIT_CANCELLED,
            message: "cancelled; no complete output was produced".to_owned(),
        }
    }
}

pub type CliResult<T> = Result<T, CliError>;
