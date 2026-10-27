//! # Secrets
//!
//! [`SecretString`] holds a credential (API key, token) and never prints it: `Debug` shows
//! `[REDACTED]` and there is no `Display`, so reading the value requires an explicit
//! [`SecretString::expose`].

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fmt;

/// A credential whose value is never printed.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct SecretString(String);

impl SecretString {
    /// Wraps a secret value.
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Returns the secret value. Use it only where the value is sent, never to log it.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Returns `true` when no value is set.
    pub fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.is_empty() {
            "[EMPTY]"
        } else {
            "[REDACTED]"
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_never_shows_the_value() {
        let secret = SecretString::new("AIza-very-secret");
        let printed = format!("{secret:?} {:?}", Some(&secret));
        assert!(!printed.contains("very-secret"), "{printed}");
        assert!(printed.contains("[REDACTED]"));
    }

    #[test]
    fn expose_returns_the_value() {
        assert_eq!(SecretString::new("abc").expose(), "abc");
    }

    #[test]
    fn blank_values_are_empty() {
        assert!(SecretString::default().is_empty());
        assert!(SecretString::new("  ").is_empty());
        assert!(!SecretString::new("x").is_empty());
        assert_eq!(format!("{:?}", SecretString::default()), "[EMPTY]");
    }
}
