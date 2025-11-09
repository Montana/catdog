use colored::*;
use std::fmt;

/// User-friendly error type that hides implementation details
#[derive(Debug)]
pub struct UserError {
    message: String,
    suggestion: Option<String>,
    exit_code: i32,
}

impl UserError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            suggestion: None,
            exit_code: 1,
        }
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn with_exit_code(mut self, code: i32) -> Self {
        self.exit_code = code;
        self
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }

    /// Display the error to the user with nice formatting
    pub fn display(&self) {
        eprintln!("{} {}", "Error:".red().bold(), self.message);

        if let Some(suggestion) = &self.suggestion {
            eprintln!("\n{} {}", "Suggestion:".yellow().bold(), suggestion);
        }
    }
}

impl fmt::Display for UserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for UserError {}

/// Convert from anyhow::Error to UserError with better messages
pub fn to_user_error(error: anyhow::Error) -> UserError {
    let error_str = error.to_string();

    // Detect common errors and provide helpful suggestions
    if error_str.contains("Permission denied") {
        return UserError::new("Permission denied")
            .with_suggestion("Try running with sudo: sudo catdog <command>")
            .with_exit_code(13);
    }

    if error_str.contains("No such file or directory") {
        if error_str.contains("/etc/fstab") {
            return UserError::new("File /etc/fstab not found")
                .with_suggestion(
                    "Your system might not use /etc/fstab. Check your OS documentation.",
                )
                .with_exit_code(2);
        }
        return UserError::new(format!("File or directory not found: {}", error_str))
            .with_suggestion("Check that the file path is correct")
            .with_exit_code(2);
    }

    if error_str.contains("Failed to run lsblk") {
        return UserError::new("Could not run lsblk command")
            .with_suggestion(
                "Install lsblk (util-linux package) or use a different device discovery method",
            )
            .with_exit_code(127);
    }

    if error_str.contains("Failed to run diskutil") {
        return UserError::new("Could not run diskutil command")
            .with_suggestion("This command requires macOS. On Linux, use lsblk instead.")
            .with_exit_code(127);
    }

    if error_str.contains("Failed to parse") {
        return UserError::new(format!("Invalid format: {}", error_str))
            .with_suggestion("Check that the file is properly formatted")
            .with_exit_code(65);
    }

    if error_str.contains("config") || error_str.contains("configuration") {
        return UserError::new(format!("Configuration error: {}", error_str))
            .with_suggestion(format!(
                "Check your config file at: {}",
                crate::config::Config::display_path()
            ))
            .with_exit_code(78);
    }

    // Generic error
    UserError::new(error_str).with_exit_code(1)
}

/// Exit code constants following sysexits.h convention
#[allow(dead_code)]
pub mod exit_codes {
    pub const SUCCESS: i32 = 0;
    pub const GENERAL_ERROR: i32 = 1;
    pub const NO_SUCH_FILE: i32 = 2;
    pub const PERMISSION_DENIED: i32 = 13;
    pub const DATA_ERROR: i32 = 65;
    pub const CONFIG_ERROR: i32 = 78;
    pub const COMMAND_NOT_FOUND: i32 = 127;
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

    #[test]
    fn test_user_error_creation() {
        let err = UserError::new("Something went wrong");
        assert_eq!(err.to_string(), "Something went wrong");
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn test_user_error_with_suggestion() {
        let err = UserError::new("File not found").with_suggestion("Check the path");
        assert!(err.suggestion.is_some());
    }

    #[test]
    fn test_permission_denied_detection() {
        let anyhow_err = anyhow!("Permission denied while accessing /etc/fstab");
        let user_err = to_user_error(anyhow_err);
        assert_eq!(user_err.exit_code(), 13);
        assert!(user_err.suggestion.is_some());
    }

    #[test]
    fn test_file_not_found_detection() {
        let anyhow_err = anyhow!("No such file or directory: /etc/fstab");
        let user_err = to_user_error(anyhow_err);
        assert_eq!(user_err.exit_code(), 2);
    }

    #[test]
    fn test_command_not_found_detection() {
        let anyhow_err = anyhow!("Failed to run lsblk command");
        let user_err = to_user_error(anyhow_err);
        assert_eq!(user_err.exit_code(), 127);
    }
}
