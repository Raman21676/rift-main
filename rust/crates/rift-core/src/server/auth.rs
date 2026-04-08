//! Authentication for remote control

use std::time::{SystemTime, UNIX_EPOCH};

/// Authentication manager with secure token validation
pub struct AuthManager {
    token: String,
}

impl AuthManager {
    /// Create new auth manager with the given token
    pub fn new(token: String) -> Self {
        Self { token }
    }

    /// Validate a provided token against the stored token
    /// Uses constant-time comparison to prevent timing attacks
    pub fn validate(&self, provided: &str) -> bool {
        if provided.len() != self.token.len() {
            return false;
        }
        // Constant-time comparison
        provided.bytes().zip(self.token.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b)) == 0
    }

    /// Get the token (for QR code generation)
    pub fn token(&self) -> &str {
        &self.token
    }
}

/// Generate a secure 32-character alphanumeric token
/// Uses only URL-safe characters (A-Z, 2-9, excluding confusing chars)
pub fn generate_token() -> String {
    use rand::Rng;
    // Exclude confusing characters: I, L, O, 0, 1
    let charset: Vec<char> = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"
        .chars().collect();
    let mut rng = rand::thread_rng();
    (0..32).map(|_| charset[rng.gen_range(0..charset.len())]).collect()
}

/// Connection info for QR code
#[derive(serde::Serialize)]
pub struct ConnectionInfo {
    pub version: String,
    pub host: String,
    pub port: u16,
    pub token: String,
    pub public_ip: Option<String>,
}

impl ConnectionInfo {
    /// Generate QR code string for this connection info
    pub fn to_qr_string(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Get local IP address
pub fn get_local_ip() -> anyhow::Result<std::net::Ipv4Addr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")?;
    socket.connect("8.8.8.8:80")?;
    match socket.local_addr()?.ip() {
        std::net::IpAddr::V4(ip) => Ok(ip),
        _ => anyhow::bail!("No IPv4 address found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_token_generation() {
        let token = generate_token();
        assert_eq!(token.len(), 32);
        // Verify only allowed characters
        let allowed: std::collections::HashSet<char> = 
            "ABCDEFGHJKLMNPQRSTUVWXYZ23456789".chars().collect();
        for c in token.chars() {
            assert!(allowed.contains(&c), "Invalid character in token: {}", c);
        }
    }

    #[test]
    fn test_auth_validation() {
        let auth = AuthManager::new("TESTTOKEN123".to_string());
        assert!(auth.validate("TESTTOKEN123"));
        assert!(!auth.validate("WRONGTOKEN"));
        assert!(!auth.validate("TESTTOKEN124"));
        assert!(!auth.validate(""));
    }
}
