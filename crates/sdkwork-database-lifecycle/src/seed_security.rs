//! Seed script security validation.
//!
//! This module provides security checks for seed scripts before execution,
//! including detection of potentially dangerous SQL operations and validation
//! of SQL syntax.

use std::path::Path;

use crate::error::LifecycleError;

/// Dangerous SQL keywords that should not appear in seed scripts.
const DANGEROUS_KEYWORDS: &[&str] = &[
    "DROP DATABASE",
    "CREATE DATABASE",
    "ALTER DATABASE",
    "GRANT",
    "REVOKE",
    "CREATE ROLE",
    "ALTER ROLE",
    "DROP ROLE",
    "CREATE USER",
    "ALTER USER",
    "DROP USER",
    "TRUNCATE",
];

/// SQL injection patterns to detect in seed scripts.
const INJECTION_PATTERNS: &[&str] = &[
    "--",  // SQL comment
    "/*",  // Block comment start
    "*/",  // Block comment end
    ";--", // Comment after statement
    "UNION SELECT",
    "UNION ALL SELECT",
    "EXEC(",
    "EXECUTE(",
    "XP_", // Extended procedures
];

/// Security validation result for a seed script.
#[derive(Debug, Clone)]
pub struct SeedSecurityReport {
    /// Whether the seed script passed all security checks.
    pub is_safe: bool,
    /// List of security warnings found.
    pub warnings: Vec<SecurityWarning>,
    /// List of critical issues that block execution.
    pub errors: Vec<SecurityError>,
}

/// A security warning (non-blocking).
#[derive(Debug, Clone)]
pub struct SecurityWarning {
    pub line: usize,
    pub message: String,
    pub suggestion: String,
}

/// A critical security error (blocks execution).
#[derive(Debug, Clone)]
pub struct SecurityError {
    pub line: usize,
    pub message: String,
}

impl SeedSecurityReport {
    /// Create a new empty report.
    pub fn new() -> Self {
        Self {
            is_safe: true,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Add a warning to the report.
    pub fn add_warning(&mut self, line: usize, message: String, suggestion: String) {
        self.warnings.push(SecurityWarning {
            line,
            message,
            suggestion,
        });
    }

    /// Add an error to the report (makes it unsafe).
    pub fn add_error(&mut self, line: usize, message: String) {
        self.is_safe = false;
        self.errors.push(SecurityError { line, message });
    }
}

impl Default for SeedSecurityReport {
    fn default() -> Self {
        Self::new()
    }
}

/// Validate a seed script for security issues.
///
/// # Security Checks
///
/// 1. **Dangerous Keywords**: Detects DROP DATABASE, CREATE USER, etc.
/// 2. **Injection Patterns**: Detects SQL comments, UNION SELECT, etc.
/// 3. **File Path Validation**: Ensures seed file is within allowed directory.
///
/// # Example
///
/// ```rust,no_run
/// use sdkwork_database_lifecycle::seed_security::validate_seed_script;
/// use std::path::Path;
///
/// let report = validate_seed_script(Path::new("seeds/001_init.sql")).unwrap();
/// if report.is_safe {
///     // Safe to execute
/// } else {
///     // Handle security issues
///     for error in &report.errors {
///         eprintln!("Security error at line {}: {}", error.line, error.message);
///     }
/// }
/// ```
pub fn validate_seed_script(seed_path: &Path) -> Result<SeedSecurityReport, LifecycleError> {
    let content = std::fs::read_to_string(seed_path)
        .map_err(|e| LifecycleError::Seed(format!("failed to read seed file: {}", e)))?;

    validate_seed_content(&content, seed_path)
}

/// Validate seed script content directly.
pub fn validate_seed_content(
    content: &str,
    seed_path: &Path,
) -> Result<SeedSecurityReport, LifecycleError> {
    let mut report = SeedSecurityReport::new();

    // Check 1: Dangerous keywords
    check_dangerous_keywords(content, &mut report);

    // Check 2: Injection patterns
    check_injection_patterns(content, &mut report);

    // Check 3: File path validation
    check_file_path(seed_path, &mut report);

    // Check 4: Statement validation
    check_statement_balance(content, &mut report);

    Ok(report)
}

/// Check for dangerous SQL keywords.
fn check_dangerous_keywords(content: &str, report: &mut SeedSecurityReport) {
    for (line_num, line) in content.lines().enumerate() {
        let upper_line = line.to_uppercase();

        for keyword in DANGEROUS_KEYWORDS {
            if upper_line.contains(keyword) {
                report.add_error(
                    line_num + 1,
                    format!(
                        "Dangerous operation '{}' detected in seed script. \
                         Seed scripts should only contain data insertion/modification, \
                         not schema or permission changes.",
                        keyword
                    ),
                );
            }
        }

        // Warn about potentially destructive operations
        if upper_line.contains("DROP TABLE") || upper_line.contains("DROP INDEX") {
            report.add_warning(
                line_num + 1,
                format!("DROP operation detected: {}", line.trim()),
                "Consider using conditional drops (IF EXISTS) or ensuring tables are empty first."
                    .to_string(),
            );
        }
    }
}

/// Check for SQL injection patterns.
fn check_injection_patterns(content: &str, report: &mut SeedSecurityReport) {
    for (line_num, line) in content.lines().enumerate() {
        let upper_line = line.to_uppercase();

        for pattern in INJECTION_PATTERNS {
            if upper_line.contains(pattern) {
                report.add_error(
                    line_num + 1,
                    format!(
                        "Potential SQL injection pattern '{}' detected. \
                         Seed scripts should not contain SQL comments or dynamic SQL patterns.",
                        pattern
                    ),
                );
            }
        }
    }
}

/// Validate that the seed file path is safe.
fn check_file_path(path: &Path, report: &mut SeedSecurityReport) {
    let path_str = path.to_string_lossy();

    // Check for path traversal
    if path_str.contains("..") {
        report.add_error(
            0,
            "Path traversal detected in seed file path. Seed files must be within the seeds directory."
                .to_string(),
        );
    }

    // Check for suspicious file extensions
    if let Some(ext) = path.extension() {
        let ext_str = ext.to_string_lossy().to_lowercase();
        if ext_str != "sql" {
            report.add_warning(
                0,
                format!("Unexpected file extension '.{}'", ext_str),
                "Seed files should have .sql extension.".to_string(),
            );
        }
    }
}

/// Check for balanced statements (basic SQL validation).
fn check_statement_balance(content: &str, report: &mut SeedSecurityReport) {
    let mut paren_depth = 0i32;
    let mut in_string = false;
    let mut string_char = ' ';

    for (line_num, line) in content.lines().enumerate() {
        let mut prev_char = ' ';

        for ch in line.chars() {
            // Handle string literals
            if (ch == '\'' || ch == '"') && prev_char != '\\' {
                if !in_string {
                    in_string = true;
                    string_char = ch;
                } else if ch == string_char {
                    in_string = false;
                }
            }

            // Track parentheses outside strings
            if !in_string {
                if ch == '(' {
                    paren_depth += 1;
                } else if ch == ')' {
                    paren_depth -= 1;
                    if paren_depth < 0 {
                        report.add_error(
                            line_num + 1,
                            "Unbalanced parentheses: closing ')' without matching '('.".to_string(),
                        );
                        paren_depth = 0;
                    }
                }
            }

            prev_char = ch;
        }
    }

    if paren_depth > 0 {
        report.add_error(
            content.lines().count(),
            format!(
                "Unbalanced parentheses: {} unclosed '(' in seed script.",
                paren_depth
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_safe_seed_content() {
        let content = "INSERT INTO users (id, name) VALUES (1, 'Alice');";
        let report = validate_seed_content(content, &PathBuf::from("seeds/test.sql")).unwrap();
        assert!(report.is_safe);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn test_dangerous_drop_database() {
        let content = "DROP DATABASE mydb;";
        let report = validate_seed_content(content, &PathBuf::from("seeds/test.sql")).unwrap();
        assert!(!report.is_safe);
        assert!(!report.errors.is_empty());
    }

    #[test]
    fn test_injection_pattern_detected() {
        let content = "SELECT * FROM users; -- malicious comment";
        let report = validate_seed_content(content, &PathBuf::from("seeds/test.sql")).unwrap();
        assert!(!report.is_safe);
    }

    #[test]
    fn test_path_traversal_detected() {
        let content = "INSERT INTO test VALUES (1);";
        let report =
            validate_seed_content(content, &PathBuf::from("../../etc/passwd.sql")).unwrap();
        assert!(!report.is_safe);
    }

    #[test]
    fn test_unbalanced_parentheses() {
        let content = "INSERT INTO test (id VALUES (1);";
        let report = validate_seed_content(content, &PathBuf::from("seeds/test.sql")).unwrap();
        assert!(!report.is_safe);
    }

    #[test]
    fn test_warning_for_drop_table() {
        let content = "DROP TABLE IF EXISTS test;";
        let report = validate_seed_content(content, &PathBuf::from("seeds/test.sql")).unwrap();
        assert!(report.is_safe); // Warning, not error
        assert!(!report.warnings.is_empty());
    }
}
