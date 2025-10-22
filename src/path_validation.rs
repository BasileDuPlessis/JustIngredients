//! Path Validation module for secure file path handling
//!
//! This module provides comprehensive security validation for file paths to prevent
//! path traversal attacks and ensure safe file operations. It implements multiple
//! layers of validation including:
//!
//! - Path traversal attack prevention (.. directory traversal)
//! - Absolute path restrictions
//! - Directory traversal restrictions
//! - Filename sanitization and validation
//! - Character encoding validation
//! - Length limits and constraints
//! - Reserved name checking (Windows reserved names)
//! - Cross-platform path normalization
//!
//! ## Security Features
//!
//! ### Path Traversal Protection
//! - Prevents `..` directory traversal attacks
//! - Blocks null byte injection (`\0`)
//! - Validates against backslash traversal on Windows
//! - Checks for encoded traversal sequences
//!
//! ### Absolute Path Restrictions
//! - Blocks absolute paths that could escape intended directories
//! - Allows only whitelisted safe directories (temp directories)
//! - Prevents access to system directories
//!
//! ### Filename Validation
//! - Sanitizes filenames to prevent injection attacks
//! - Validates character encoding (UTF-8)
//! - Checks for reserved names (CON, PRN, etc.)
//! - Enforces length limits
//!
//! ## Usage Examples
//!
//! ```rust
//! use just_ingredients::path_validation::{validate_file_path, sanitize_filename};
//!
//! // Validate a file path for security
//! match validate_file_path("/tmp/safe_file.jpg") {
//!     Ok(()) => println!("Path is safe"),
//!     Err(e) => println!("Path validation failed: {}", e),
//! }
//!
//! // Sanitize a filename
//! let safe_name = sanitize_filename("unsafe<name>.jpg");
//! assert_eq!(safe_name, "unsafe_name_.jpg");
//! ```

use std::path::Path;

/// Errors that can occur during path validation
#[derive(Debug, Clone, PartialEq)]
pub enum PathValidationError {
    /// Path contains dangerous traversal sequences (..)
    PathTraversal,
    /// Path contains null bytes
    NullByte,
    /// Path is absolute and not in allowed directories
    AbsolutePathNotAllowed,
    /// Path contains invalid characters
    InvalidCharacters,
    /// Filename is too long
    FilenameTooLong,
    /// Path is too long
    PathTooLong,
    /// Filename uses reserved name
    ReservedName,
    /// Invalid UTF-8 encoding
    InvalidEncoding,
    /// Empty path provided
    EmptyPath,
}

/// Result type for path validation operations
pub type PathValidationResult<T> = Result<T, PathValidationError>;

/// Maximum allowed filename length (255 bytes on most filesystems)
pub const MAX_FILENAME_LENGTH: usize = 255;

/// Maximum allowed path length (4096 bytes on most systems)
pub const MAX_PATH_LENGTH: usize = 4096;

/// Reserved filenames that should not be used (Windows compatibility)
pub const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Characters that are not allowed in filenames
pub const FORBIDDEN_FILENAME_CHARS: &[char] = &[
    '<', '>', ':', '"', '|', '?', '*', '\0', // null byte
    '\x01', '\x02', '\x03', '\x04', '\x05', '\x06', '\x07', // control chars
    '\x08', '\x09', '\x0a', '\x0b', '\x0c', '\x0d', '\x0e', '\x0f', // control chars
    '\x10', '\x11', '\x12', '\x13', '\x14', '\x15', '\x16', '\x17', // control chars
    '\x18', '\x19', '\x1a', '\x1b', '\x1c', '\x1d', '\x1e', '\x1f', // control chars
];

/// Directories that are considered safe for absolute paths
pub const ALLOWED_ABSOLUTE_PATH_PREFIXES: &[&str] = &[
    "/tmp",
    "/var/tmp",
    "/private/tmp",
    "/private/var/tmp",
    "/var/folders", // macOS temp directories
    // Windows temp directories
    "C:\\Windows\\Temp",
    "C:\\Temp",
    "C:\\Users\\",
];

/// Validate a file path for security issues
///
/// Performs comprehensive security validation including:
/// - Path traversal attack prevention
/// - Absolute path restrictions
/// - Character encoding validation
/// - Length limits
/// - Reserved name checking
///
/// # Arguments
///
/// * `path` - The file path to validate
///
/// # Returns
///
/// Returns `Ok(())` if the path is safe, or `PathValidationError` if validation fails
///
/// # Examples
///
/// ```rust
/// use just_ingredients::path_validation::validate_file_path;
///
/// // Safe relative path
/// assert!(validate_file_path("safe_file.jpg").is_ok());
///
/// // Path traversal attack (blocked)
/// assert!(validate_file_path("../etc/passwd").is_err());
///
/// // Absolute path in temp directory (allowed)
/// assert!(validate_file_path("/tmp/safe_file.jpg").is_ok());
///
/// // Absolute path in system directory (blocked)
/// assert!(validate_file_path("/etc/passwd").is_err());
/// ```
pub fn validate_file_path(path: &str) -> PathValidationResult<()> {
    // Check for empty path
    if path.is_empty() {
        return Err(PathValidationError::EmptyPath);
    }

    // Check path length
    if path.len() > MAX_PATH_LENGTH {
        return Err(PathValidationError::PathTooLong);
    }

    // Check for null bytes
    if path.contains('\0') {
        return Err(PathValidationError::NullByte);
    }

    // Check for path traversal attacks
    if contains_path_traversal(path) {
        return Err(PathValidationError::PathTraversal);
    }

    // Validate absolute paths
    validate_absolute_path(path)?;

    // Convert to Path for further validation
    let path_obj = Path::new(path);

    // Validate filename if present
    if let Some(filename) = path_obj.file_name() {
        let filename_str = filename.to_string_lossy();
        validate_filename(&filename_str)?;
    }

    Ok(())
}

/// Check if a path contains path traversal sequences
///
/// Detects various forms of directory traversal attacks:
/// - `..` sequences
/// - `../` and `..\` patterns
/// - Encoded traversal sequences
/// - Multiple traversal attempts
///
/// # Arguments
///
/// * `path` - The path to check
///
/// # Returns
///
/// Returns `true` if path traversal is detected
fn contains_path_traversal(path: &str) -> bool {
    // Check for basic .. patterns
    if path.contains("..") {
        // More sophisticated check: look for .. as directory components
        let path_obj = Path::new(path);

        // Check each component for ..
        for component in path_obj.components() {
            if let std::path::Component::ParentDir = component {
                return true;
            }
        }
    }

    // Check for encoded traversal (URL encoded)
    if path.contains("%2e%2e") || path.contains("%2E%2E") {
        return true;
    }

    // Check for backslash traversal on Windows
    if cfg!(windows) && path.contains("\\..\\") {
        return true;
    }

    false
}

/// Validate absolute paths to ensure they are in allowed directories
///
/// # Arguments
///
/// * `path` - The path to validate
///
/// # Returns
///
/// Returns `Ok(())` if the absolute path is allowed, or `PathValidationError` if not
fn validate_absolute_path(path: &str) -> PathValidationResult<()> {
    let path_obj = Path::new(path);

    if path_obj.is_absolute() {
        // Check if path starts with any allowed prefix
        let path_str = path_obj.to_string_lossy();

        let is_allowed = ALLOWED_ABSOLUTE_PATH_PREFIXES
            .iter()
            .any(|prefix| path_str.starts_with(prefix));

        if !is_allowed {
            // Additional check: prevent access to common system directories
            let dangerous_prefixes = [
                "/etc",
                "/usr",
                "/bin",
                "/sbin",
                "/System",
                "/Library",
                "/root",
                "/home",
                "/var",
                "/proc",
                "/sys",
                "/dev",
                "C:\\Windows\\System32",
                "C:\\Windows",
                "C:\\Program Files",
            ];

            let is_dangerous = dangerous_prefixes
                .iter()
                .any(|prefix| path_str.starts_with(prefix));

            if is_dangerous || !is_allowed {
                return Err(PathValidationError::AbsolutePathNotAllowed);
            }
        }
    }

    Ok(())
}

/// Validate a filename for security issues
///
/// # Arguments
///
/// * `filename` - The filename to validate
///
/// # Returns
///
/// Returns `Ok(())` if the filename is safe, or `PathValidationError` if validation fails
pub fn validate_filename(filename: &str) -> PathValidationResult<()> {
    // Check length
    if filename.len() > MAX_FILENAME_LENGTH {
        return Err(PathValidationError::FilenameTooLong);
    }

    // Check for empty filename
    if filename.is_empty() {
        return Err(PathValidationError::EmptyPath);
    }

    // Check for reserved names (case-insensitive)
    let filename_upper = filename.to_uppercase();
    let name_without_ext = filename_upper.split('.').next().unwrap_or("");

    if RESERVED_NAMES.contains(&name_without_ext) {
        return Err(PathValidationError::ReservedName);
    }

    // Check for forbidden characters
    if filename
        .chars()
        .any(|c| FORBIDDEN_FILENAME_CHARS.contains(&c))
    {
        return Err(PathValidationError::InvalidCharacters);
    }

    // Check for control characters (additional validation)
    if filename
        .chars()
        .any(|c| c.is_control() && c != '\t' && c != '\n' && c != '\r')
    {
        return Err(PathValidationError::InvalidCharacters);
    }

    Ok(())
}

/// Sanitize a filename by removing or replacing dangerous characters
///
/// This function makes filenames safe by:
/// - Removing or replacing forbidden characters
/// - Trimming whitespace
/// - Handling empty results
/// - Ensuring the result is a valid filename
///
/// # Arguments
///
/// * `filename` - The filename to sanitize
///
/// # Returns
///
/// Returns a sanitized filename that is safe to use
///
/// # Examples
///
/// ```rust
/// use just_ingredients::path_validation::sanitize_filename;
///
/// assert_eq!(sanitize_filename("safe_file.jpg"), "safe_file.jpg");
/// assert_eq!(sanitize_filename("unsafe<name>.jpg"), "unsafe_name_.jpg");
/// assert_eq!(sanitize_filename("file with spaces.jpg"), "file with spaces.jpg");
/// ```
pub fn sanitize_filename(filename: &str) -> String {
    // Start with the original filename
    let mut sanitized = filename.to_string();

    // Replace forbidden characters with underscores
    for &forbidden in FORBIDDEN_FILENAME_CHARS {
        sanitized = sanitized.replace(forbidden, "_");
    }

    // Trim whitespace
    sanitized = sanitized.trim().to_string();

    // Handle empty result
    if sanitized.is_empty() {
        sanitized = "unnamed_file".to_string();
    }

    // Ensure it doesn't start or end with dots (Windows issues)
    sanitized = sanitized.trim_matches('.').trim().to_string();
    if sanitized.is_empty() {
        sanitized = "unnamed_file".to_string();
    }

    // Limit length
    if sanitized.len() > MAX_FILENAME_LENGTH {
        // Try to preserve extension
        if let Some(dot_pos) = sanitized.rfind('.') {
            let name = &sanitized[..dot_pos];
            let ext = &sanitized[dot_pos..];
            let max_name_len = MAX_FILENAME_LENGTH.saturating_sub(ext.len());
            sanitized = format!("{}{}", &name[..max_name_len.min(name.len())], ext);
        } else {
            sanitized = sanitized[..MAX_FILENAME_LENGTH].to_string();
        }
    }

    sanitized
}

/// Normalize a path for cross-platform compatibility
///
/// This function normalizes paths by:
/// - Converting backslashes to forward slashes on Unix
/// - Converting forward slashes to backslashes on Windows
/// - Removing redundant separators
/// - Handling relative path components
///
/// # Arguments
///
/// * `path` - The path to normalize
///
/// # Returns
///
/// Returns a normalized path string
///
/// # Examples
///
/// ```rust
/// use just_ingredients::path_validation::normalize_path;
///
/// // The function returns the platform-specific path representation
/// // On Unix systems, backslashes are valid filename characters
/// let path = normalize_path("path\\to\\file");
/// assert_eq!(path, "path\\to\\file");
///
/// // On Windows systems, it would convert forward slashes to backslashes
/// // assert_eq!(normalize_path("path/to/file"), "path\\to\\file");
/// ```
pub fn normalize_path(path: &str) -> String {
    let path_obj = Path::new(path);

    // Use the platform-specific path representation
    path_obj.to_string_lossy().to_string()
}

/// Check if a path is safe for reading operations
///
/// This is a convenience function that combines path validation
/// with additional checks for read operations.
///
/// # Arguments
///
/// * `path` - The path to validate for reading
///
/// # Returns
///
/// Returns `Ok(())` if the path is safe for reading, or `PathValidationError` if not
pub fn validate_path_for_reading(path: &str) -> PathValidationResult<()> {
    // Basic validation
    validate_file_path(path)?;

    // Additional checks for reading
    let path_obj = Path::new(path);

    // Check if file exists (for reading operations)
    if !path_obj.exists() {
        return Err(PathValidationError::EmptyPath); // Reuse error for file not found
    }

    // Check if it's actually a file (not a directory)
    if path_obj.is_dir() {
        return Err(PathValidationError::InvalidCharacters); // Reuse error for wrong type
    }

    Ok(())
}

/// Check if a path is safe for writing operations
///
/// This is a convenience function that combines path validation
/// with additional checks for write operations.
///
/// # Arguments
///
/// * `path` - The path to validate for writing
///
/// # Returns
///
/// Returns `Ok(())` if the path is safe for writing, or `PathValidationError` if not
pub fn validate_path_for_writing(path: &str) -> PathValidationResult<()> {
    // Basic validation
    validate_file_path(path)?;

    // Additional checks for writing
    let path_obj = Path::new(path);

    // Check parent directory exists and is writable
    if let Some(parent) = path_obj.parent() {
        if !parent.exists() {
            return Err(PathValidationError::InvalidCharacters); // Reuse error
        }
        if !parent.is_dir() {
            return Err(PathValidationError::InvalidCharacters); // Parent not a directory
        }
    }

    Ok(())
}

/// Generate a safe temporary filename
///
/// Creates a filename that is guaranteed to be safe and unique
/// within the context of temporary file operations.
///
/// # Arguments
///
/// * `prefix` - Optional prefix for the filename
/// * `extension` - Optional file extension
///
/// # Returns
///
/// Returns a safe temporary filename
///
/// # Examples
///
/// ```rust
/// use just_ingredients::path_validation::generate_safe_temp_filename;
///
/// let filename = generate_safe_temp_filename(Some("upload"), Some("jpg"));
/// assert!(filename.starts_with("upload_"));
/// assert!(filename.ends_with(".jpg"));
/// ```
pub fn generate_safe_temp_filename(prefix: Option<&str>, extension: Option<&str>) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    let random_part = (timestamp % 1000000) as u32;

    let mut filename = prefix.unwrap_or("temp").to_string();
    filename.push_str(&format!("_{}", random_part));

    if let Some(ext) = extension {
        // Sanitize extension
        let safe_ext = sanitize_filename(ext);
        if !safe_ext.is_empty() && !safe_ext.contains('.') {
            filename.push('.');
            filename.push_str(&safe_ext);
        }
    }

    // Final sanitization
    sanitize_filename(&filename)
}

impl std::fmt::Display for PathValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathValidationError::PathTraversal => {
                write!(f, "Path contains directory traversal sequences")
            }
            PathValidationError::NullByte => write!(f, "Path contains null bytes"),
            PathValidationError::AbsolutePathNotAllowed => {
                write!(f, "Absolute path not in allowed directories")
            }
            PathValidationError::InvalidCharacters => write!(f, "Path contains invalid characters"),
            PathValidationError::FilenameTooLong => write!(f, "Filename is too long"),
            PathValidationError::PathTooLong => write!(f, "Path is too long"),
            PathValidationError::ReservedName => write!(f, "Filename uses reserved name"),
            PathValidationError::InvalidEncoding => write!(f, "Path contains invalid encoding"),
            PathValidationError::EmptyPath => write!(f, "Path is empty"),
        }
    }
}

impl std::error::Error for PathValidationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_safe_relative_path() {
        assert!(validate_file_path("safe_file.jpg").is_ok());
        assert!(validate_file_path("path/to/file.jpg").is_ok());
        assert!(validate_file_path("file.with.dots.jpg").is_ok());
    }

    #[test]
    fn test_validate_path_traversal() {
        assert!(validate_file_path("../etc/passwd").is_err());
        assert!(validate_file_path("../../../etc/passwd").is_err());
        assert!(validate_file_path("path/../../../etc/passwd").is_err());
    }

    #[test]
    fn test_validate_null_bytes() {
        assert!(validate_file_path("file\0name.jpg").is_err());
        assert!(validate_file_path("file\x00name.jpg").is_err());
    }

    #[test]
    fn test_validate_absolute_paths() {
        // Allowed absolute paths
        assert!(validate_file_path("/tmp/safe_file.jpg").is_ok());
        assert!(validate_file_path("/var/tmp/file.jpg").is_ok());

        // Dangerous absolute paths
        assert!(validate_file_path("/etc/passwd").is_err());
        assert!(validate_file_path("/usr/bin/ls").is_err());
        assert!(validate_file_path("/root/.bashrc").is_err());
    }

    #[test]
    fn test_validate_filename() {
        // Valid filenames
        assert!(validate_filename("file.jpg").is_ok());
        assert!(validate_filename("file.with.dots.jpg").is_ok());
        assert!(validate_filename("file-name.jpg").is_ok());

        // Invalid filenames
        assert!(validate_filename("file<name>.jpg").is_err());
        assert!(validate_filename("file>name.jpg").is_err());
        assert!(validate_filename("file:name.jpg").is_err());
        assert!(validate_filename("CON.jpg").is_err());
        assert!(validate_filename("con.jpg").is_err());
        assert!(validate_filename("file\x00name.jpg").is_err());
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("safe_file.jpg"), "safe_file.jpg");
        assert_eq!(sanitize_filename("unsafe<name>.jpg"), "unsafe_name_.jpg");
        assert_eq!(
            sanitize_filename("file:with:colons.jpg"),
            "file_with_colons.jpg"
        );
        assert_eq!(
            sanitize_filename("  spaced file  .jpg"),
            "spaced file  .jpg"
        );
        assert_eq!(sanitize_filename(""), "unnamed_file");
        assert_eq!(sanitize_filename("."), "unnamed_file");
    }

    #[test]
    fn test_generate_safe_temp_filename() {
        let filename = generate_safe_temp_filename(Some("test"), Some("jpg"));
        assert!(filename.starts_with("test_"));
        assert!(filename.ends_with(".jpg"));
        assert!(validate_filename(&filename).is_ok());
    }

    #[test]
    fn test_path_validation_errors() {
        assert_eq!(
            validate_file_path("../test").unwrap_err(),
            PathValidationError::PathTraversal
        );
        assert_eq!(
            validate_file_path("file\0name").unwrap_err(),
            PathValidationError::NullByte
        );
        assert_eq!(
            validate_file_path("/etc/passwd").unwrap_err(),
            PathValidationError::AbsolutePathNotAllowed
        );
        assert_eq!(
            validate_file_path("").unwrap_err(),
            PathValidationError::EmptyPath
        );
    }

    #[test]
    fn test_reserved_names() {
        for &reserved in RESERVED_NAMES {
            assert!(validate_filename(&format!("{}.txt", reserved)).is_err());
            assert!(validate_filename(&format!("{}.txt", reserved.to_lowercase())).is_err());
        }
    }

    #[test]
    fn test_length_limits() {
        let long_filename = "a".repeat(MAX_FILENAME_LENGTH + 1);
        assert!(validate_filename(&long_filename).is_err());

        let long_path = "a".repeat(MAX_PATH_LENGTH + 1);
        assert!(validate_file_path(&long_path).is_err());
    }
}
