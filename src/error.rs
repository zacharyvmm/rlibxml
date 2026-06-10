//! libxml2 error types and helpers.
//!
//! Provides safe wrappers around libxml2's error reporting API so callers
//! can inspect parse errors instead of receiving a silent `None`.

use std::ffi::CStr;
use std::fmt;

use crate::ffi;

/// Severity level of a libxml2 error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorLevel {
    None_,
    Warning,
    Error,
    Fatal,
}

impl fmt::Display for ErrorLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorLevel::None_ => write!(f, "none"),
            ErrorLevel::Warning => write!(f, "warning"),
            ErrorLevel::Error => write!(f, "error"),
            ErrorLevel::Fatal => write!(f, "fatal"),
        }
    }
}

/// A structured libxml2 error.
///
/// Obtained by calling [`XmlError::last`] after a parse operation that
/// returned `None` or `Err`.
#[derive(Debug, Clone)]
pub struct XmlError {
    pub domain: i32,
    pub code: i32,
    pub message: String,
    pub level: ErrorLevel,
    pub file: Option<String>,
    pub line: i32,
}

impl XmlError {
    /// Read the last error from libxml2's thread-local error state.
    ///
    /// Returns `None` if no error has been recorded (or was already
    /// consumed by a previous call to this function).
    ///
    /// This function also resets the error state so that subsequent
    /// calls don't return the same error.
    pub fn last() -> Option<Self> {
        unsafe {
            let ptr = ffi::xmlGetLastError();
            if ptr.is_null() {
                return None;
            }

            let err = &*ptr;
            let message = if err.message.is_null() {
                String::from("unknown error")
            } else {
                CStr::from_ptr(err.message)
                    .to_string_lossy()
                    .into_owned()
            };

            let file = if err.file.is_null() {
                None
            } else {
                Some(
                    CStr::from_ptr(err.file)
                        .to_string_lossy()
                        .into_owned(),
                )
            };

            let level = match err.level {
                ffi::xmlErrorLevel_XML_ERR_NONE => ErrorLevel::None_,
                ffi::xmlErrorLevel_XML_ERR_WARNING => ErrorLevel::Warning,
                ffi::xmlErrorLevel_XML_ERR_ERROR => ErrorLevel::Error,
                ffi::xmlErrorLevel_XML_ERR_FATAL => ErrorLevel::Fatal,
                _ => ErrorLevel::Error,
            };

            // Reset the error so it isn't reported again
            ffi::xmlResetLastError();

            Some(XmlError {
                domain: err.domain,
                code: err.code,
                message,
                level,
                file,
                line: err.line,
            })
        }
    }

    /// Clear any pending libxml2 errors without reading them.
    pub fn reset() {
        unsafe { ffi::xmlResetLastError() };
    }
}

impl fmt::Display for XmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "libxml2 {} (domain={}, code={}): {}",
            self.level, self.domain, self.code, self.message
        )?;
        if let Some(ref file) = self.file {
            write!(f, " at {}:{}", file, self.line)?;
        }
        Ok(())
    }
}

impl std::error::Error for XmlError {}
