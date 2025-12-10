use anyhow::Result;
use mdns_sd::{ServiceDaemon, ServiceInfo, ServiceEvent};
use log::{info, error, warn, debug};
use uuid::Uuid;
use std::sync::Arc;
use crate::peers::PeerManager;
use std::net::SocketAddr;
use std::str::FromStr;

use crate::blocks::InMemoryBlockManager;

pub struct MdnsDiscovery {
    daemon: ServiceDaemon,
    service_type: &'static str,
    node_id: Uuid,
    port: u16,
    peer_manager: Arc<PeerManager>,
    block_manager: Arc<InMemoryBlockManager>,
    default_quota: u64,
}

impl MdnsDiscovery {
    pub fn new(node_id: Uuid, port: u16, peer_manager: Arc<PeerManager>, block_manager: Arc<InMemoryBlockManager>, default_quota: u64) -> Result<Self> {
        let daemon = ServiceDaemon::new().map_err(|e| {
            error!("Failed to create mDNS daemon: {}. Auto-discovery will not work.", e);
            error!("This may be due to: firewall blocking port 5353, another mDNS service running, or network restrictions.");
            e
        })?;
        
        Ok(Self {
            daemon,
            service_type: "_memcloud._tcp.local.",
            node_id,
            port,
            peer_manager,
            block_manager,
            default_quota,
        })
    }

    pub fn start_advertising(&self) -> Result<()> {
        let hostname = format!("memcloud-{}", self.node_id);
        let properties = [("id", self.node_id.to_string())];
        
        let my_service = ServiceInfo::new(
            self.service_type,
            &self.node_id.to_string(), // instance name
            &hostname,
            "", // ip - let mdns-sd auto-detect
            self.port,
            match std::collections::HashMap::from_iter(properties.iter().map(|(k, v)| (k.to_string(), v.to_string()))) {
                 props => Some(props)
            },
        ).map_err(|e| {
            error!("Failed to create mDNS service info: {}", e);
            e
        })?;

        self.daemon.register(my_service).map_err(|e| {
            error!("Failed to register mDNS service: {}. Other devices won't discover this node.", e);
            e
        })?;
        
        info!("✅ mDNS advertising started for {} on port {}", self.node_id, self.port);
        info!("   Service type: {}", self.service_type);
        Ok(())
    }

    pub fn start_browsing(&self) -> Result<()> {
        let receiver = self.daemon.browse(self.service_type).map_err(|e| {
            error!("Failed to start mDNS browsing: {}. Auto-discovery will not work.", e);
            error!("Possible causes: firewall, network restrictions, or mDNS daemon issue.");
            e
        })?;
        
        let my_id = self.node_id;
        let peer_manager = self.peer_manager.clone();
        let block_manager = self.block_manager.clone();
        let quota = self.default_quota;

        tokio::spawn(async move {
            info!("🔍 mDNS browser started, listening for MemCloud peers...");
            
            while let Ok(event) = receiver.recv() {
                match event {
                    ServiceEvent::ServiceFound(service_type, fullname) => {
                        debug!("mDNS ServiceFound: {} (type: {})", fullname, service_type);
                    }
                    ServiceEvent::ServiceResolved(info) => {
                        let fullname = info.get_fullname();
                        debug!("mDNS ServiceResolved: {}", fullname);
                        
                        // Check if it's our own service
                        if fullname.contains(&my_id.to_string()) {
                            debug!("Ignoring own service: {}", fullname);
                            continue;
                        }

                        // Extract peer ID from properties
                        let id_prop_raw = match info.get_property_val("id") {
                            Some(val) => val,
                            None => {
                                warn!("Discovered MemCloud service '{}' but missing 'id' property. Skipping.", fullname);
                                continue;
                            }
                        };
                        
                        // Handle Option<&[u8]> from get_property_val
                        let id_bytes = match id_prop_raw {
                            Some(b) => b,
                            None => {
                                warn!("Discovered MemCloud service '{}' has empty 'id' property. Skipping.", fullname);
                                continue;
                            }
                        };
                        
                        let id_str = match std::str::from_utf8(id_bytes) {
                            Ok(s) => s,
                            Err(e) => {
                                warn!("Discovered MemCloud service '{}' has invalid UTF-8 in 'id' property: {}. Skipping.", fullname, e);
                                continue;
                            }
                        };
                        
                        let peer_id = match Uuid::from_str(id_str) {
                            Ok(id) => id,
                            Err(e) => {
                                warn!("Discovered MemCloud service '{}' has invalid UUID '{}': {}. Skipping.", fullname, id_str, e);
                                continue;
                            }
                        };
                        
                        // Get addresses
                        let addresses = info.get_addresses();
                        if addresses.is_empty() {
                            warn!("Discovered peer {} but no IP addresses available. This may be a network configuration issue.", peer_id);
                            continue;
                        }
                        
                        // Prefer IPv4 addresses over IPv6 for compatibility
                        let addr = addresses.iter()
                            .find(|a| a.is_ipv4())
                            .or_else(|| addresses.iter().next());
                        
                        let addr = match addr {
                            Some(a) => a,
                            None => {
                                warn!("Discovered peer {} but could not select a usable IP address.", peer_id);
                                continue;
                            }
                        };
                        
                        let socket_addr = SocketAddr::new(*addr, info.get_port());
                        info!("🔗 Discovered peer {} at {}", peer_id, socket_addr);
                        
                        // Attempt to connect
                        match peer_manager.add_discovered_peer(peer_id, socket_addr, block_manager.clone(), peer_manager.clone(), quota).await {
                            Ok(_) => {
                                info!("✅ Successfully connected to discovered peer {}", peer_id);
                            }
                            Err(e) => {
                                error!("❌ Failed to connect to discovered peer {} at {}: {}", peer_id, socket_addr, e);
                            }
                        }
                    }
                    ServiceEvent::ServiceRemoved(service_type, fullname) => {
                        info!("📤 mDNS peer went offline: {} ({})", fullname, service_type);
                        // TODO: Could remove peer from peer_manager here
                    }
                    ServiceEvent::SearchStarted(service_type) => {
                        debug!("mDNS search started for: {}", service_type);
                    }
                    ServiceEvent::SearchStopped(service_type) => {
                        warn!("mDNS search stopped for: {}. Discovery may not work.", service_type);
                    }
                }
            }
            
            warn!("mDNS browser loop exited unexpectedly. Auto-discovery is no longer active.");
        });
        
        info!("✅ mDNS browsing started for service type: {}", self.service_type);
        Ok(())
    }
}
