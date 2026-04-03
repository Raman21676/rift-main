//! Capability-based permission system
//! 
//! Unlike traditional permission modes, capabilities are fine-grained tokens
//! that must be explicitly granted and can be scoped to specific resources.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// A capability represents permission to perform an action
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// Read any file
    FileRead,
    /// Read specific files (glob patterns)
    FileReadScoped(Vec<String>),
    /// Write any file
    FileWrite,
    /// Write specific files
    FileWriteScoped(Vec<String>),
    /// Execute any shell command
    ShellExecute,
    /// Execute shell commands in specific directories
    ShellExecuteScoped(Vec<String>),
    /// Access network
    NetworkAccess,
    /// Access specific hosts
    NetworkHost(String),
}

impl Capability {
    /// Check if this capability implies another
    pub fn implies(&self, other: &Capability) -> bool {
        match (self, other) {
            // Exact match
            (a, b) if a == b => true,
            
            // Scoped read implies specific file patterns
            (Capability::FileRead, Capability::FileReadScoped(_)) => true,
            (Capability::FileReadScoped(a), Capability::FileReadScoped(b)) => {
                b.iter().all(|pattern| a.iter().any(|p| p == pattern))
            }
            
            // Scoped write
            (Capability::FileWrite, Capability::FileWriteScoped(_)) => true,
            (Capability::FileWriteScoped(a), Capability::FileWriteScoped(b)) => {
                b.iter().all(|pattern| a.iter().any(|p| p == pattern))
            }
            
            // Shell execution
            (Capability::ShellExecute, Capability::ShellExecuteScoped(_)) => true,
            
            // Network access
            (Capability::NetworkAccess, Capability::NetworkHost(_)) => true,
            
            _ => false,
        }
    }
    
    /// Get a human-readable description
    pub fn description(&self) -> String {
        match self {
            Capability::FileRead => "Read any file".to_string(),
            Capability::FileReadScoped(patterns) => {
                format!("Read files matching: {}", patterns.join(", "))
            }
            Capability::FileWrite => "Write any file".to_string(),
            Capability::FileWriteScoped(patterns) => {
                format!("Write files matching: {}", patterns.join(", "))
            }
            Capability::ShellExecute => "Execute any shell command".to_string(),
            Capability::ShellExecuteScoped(dirs) => {
                format!("Execute commands in: {}", dirs.join(", "))
            }
            Capability::NetworkAccess => "Access any network resource".to_string(),
            Capability::NetworkHost(host) => format!("Access host: {}", host),
        }
    }
}

/// Manages granted capabilities
#[derive(Debug, Clone, Default)]
pub struct CapabilityManager {
    granted: std::collections::HashSet<Capability>,
}

impl CapabilityManager {
    /// Create a new capability manager with no permissions
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create with initial capabilities
    pub fn with_capabilities(caps: Vec<Capability>) -> Self {
        Self {
            granted: caps.into_iter().collect(),
        }
    }
    
    /// Grant a capability
    pub fn grant(&mut self, cap: Capability) {
        self.granted.insert(cap);
    }
    
    /// Revoke a capability
    pub fn revoke(&mut self, cap: &Capability) {
        self.granted.remove(cap);
    }
    
    /// Check if a capability is granted
    pub fn has(&self, required: &Capability) -> bool {
        self.granted.iter().any(|c| c.implies(required))
    }
    
    /// Check if all capabilities are granted
    pub fn has_all(&self, required: &[Capability]) -> bool {
        required.iter().all(|c| self.has(c))
    }
    
    /// Get all granted capabilities
    pub fn granted(&self) -> &HashSet<Capability> {
        &self.granted
    }
    
    /// Verify capabilities, returning error if any are missing
    pub fn verify(&self, required: &[Capability]) -> Result<(), CapabilityError> {
        let missing: Vec<_> = required
            .iter()
            .filter(|c| !self.has(c))
            .cloned()
            .collect();
        
        if missing.is_empty() {
            Ok(())
        } else {
            Err(CapabilityError::MissingCapabilities(missing))
        }
    }
}

/// Errors related to capability verification
#[derive(Debug, thiserror::Error)]
pub enum CapabilityError {
    #[error("Missing capabilities: {0:?}")]
    MissingCapabilities(Vec<Capability>),
    
    #[error("Capability denied: {0}")]
    Denied(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_implies() {
        assert!(Capability::FileRead.implies(&Capability::FileRead));
        assert!(Capability::FileRead.implies(&Capability::FileReadScoped(vec!["*.rs".to_string()])));
        assert!(!Capability::FileReadScoped(vec!["*.rs".to_string()]).implies(&Capability::FileRead));
    }

    #[test]
    fn test_capability_manager() {
        let mut manager = CapabilityManager::new();
        manager.grant(Capability::FileRead);
        
        assert!(manager.has(&Capability::FileRead));
        assert!(manager.has(&Capability::FileReadScoped(vec!["*.rs".to_string()])));
        assert!(!manager.has(&Capability::FileWrite));
    }
}
