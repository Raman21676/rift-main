//! Remote control server for Rift daemon
//!
//! Provides HTTP REST API and WebSocket endpoints for remote monitoring
//! and control of the Rift daemon from Android or other clients.

use std::sync::Arc;
use std::path::PathBuf;
use tokio::sync::RwLock;
use crate::daemon::Daemon;

mod auth;
mod rest_api;
mod websocket;

pub use auth::{AuthManager, generate_token, ConnectionInfo, get_local_ip};
pub use rest_api::AppState;

/// Remote control server for the daemon
pub struct RemoteServer {
    pub daemon: Arc<RwLock<Daemon>>,
    pub auth: Arc<AuthManager>,
    pub port: u16,
    pub public_ip: Option<String>,
}

impl RemoteServer {
    /// Create a new remote server
    pub fn new(daemon: Arc<RwLock<Daemon>>, port: u16) -> Self {
        let token = generate_token();
        let local_ip = get_local_ip().map(|ip| ip.to_string()).unwrap_or_else(|_| "127.0.0.1".to_string());
        
        println!("🔑 Remote token: {}", token);
        
        // Generate and print connection info
        let conn_info = ConnectionInfo {
            version: "1".to_string(),
            host: local_ip.clone(),
            port,
            token: token.clone(),
            public_ip: None,
        };
        
        println!("📱 Connection QR: {}", conn_info.to_qr_string());
        
        // Try to print ASCII QR code if available
        print_ascii_qr(&conn_info.to_qr_string());
        
        Self {
            daemon,
            auth: Arc::new(AuthManager::new(token)),
            port,
            public_ip: None,
        }
    }

    /// Run the remote server (blocks until shutdown)
    pub async fn run(self) -> anyhow::Result<()> {
        // Try UPnP mapping
        match try_upnp_mapping(self.port).await {
            Ok(external_ip) => {
                println!("✅ UPnP: {}:{} is publicly accessible", external_ip, self.port);
            }
            Err(e) => {
                println!("⚠️  UPnP failed (local network only): {}", e);
            }
        }

        // Start axum server
        rest_api::start(self.daemon, self.auth, self.port).await
    }
}

/// Try to map port via UPnP (blocking operation run in spawn_blocking)
async fn try_upnp_mapping(port: u16) -> anyhow::Result<String> {
    use igd::search_gateway;
    use std::net::SocketAddrV4;
    use std::time::Duration;

    tokio::task::spawn_blocking(move || {
        let gateway = search_gateway(Default::default())
            .map_err(|e| anyhow::anyhow!("UPnP gateway search failed: {}", e))?;
        
        let local_ip = get_local_ip()?;
        
        gateway.add_port(
            igd::PortMappingProtocol::TCP,
            port,
            SocketAddrV4::new(local_ip, port),
            86400,   // 24-hour lease
            "Rift Remote",
        ).map_err(|e| anyhow::anyhow!("UPnP port mapping failed: {}", e))?;
        
        let external_ip = gateway.get_external_ip()
            .map_err(|e| anyhow::anyhow!("UPnP get external IP failed: {}", e))?;
        Ok(external_ip.to_string())
    }).await?
}

/// Print ASCII QR code if possible
fn print_ascii_qr(data: &str) {
    // Simple ASCII representation - in production use a QR library
    println!("\n📲 Scan this QR code:");
    println!("┌─────────────────────────┐");
    println!("│  {}  │", &data[..20.min(data.len())]);
    println!("│  (QR code placeholder)  │");
    println!("└─────────────────────────┘\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_info_serialization() {
        let info = ConnectionInfo {
            version: "1".to_string(),
            host: "192.168.1.5".to_string(),
            port: 7788,
            token: "ABCD1234".to_string(),
            public_ip: Some("203.0.113.1".to_string()),
        };
        
        let json = info.to_qr_string();
        assert!(json.contains("192.168.1.5"));
        assert!(json.contains("7788"));
        assert!(json.contains("ABCD1234"));
    }
}
