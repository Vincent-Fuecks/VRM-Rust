use serde::de::DeserializeOwned;
use std::fs;

use crate::error::{Error, Result};

/// Parses a JSON file into a given type `T`.
///
/// This function reads a file from `file_path`, attempts to parse it
/// as JSON, and returns an instance of `T`.
///
/// Errors are automatically converted into `crate::error::Error` variants:
/// - `Error::IoError` if the file cannot be read.
/// - `Error::DeserializationError` if the JSON is malformed.
pub fn parse_json_file<T: DeserializeOwned>(file_path: &str) -> Result<T> {
    let data = fs::read_to_string(file_path).map_err(Error::IoError)?;

    let parsed_data: T = serde_json::from_str(&data).map_err(Error::DeserializationError)?;

    Ok(parsed_data)
}
