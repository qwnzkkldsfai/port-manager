use netstat2::{
    get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo, TcpState,
};
use serde::Serialize;
use std::collections::HashMap;
use sysinfo::System;

#[derive(Debug, Clone, Serialize)]
pub struct ListeningEntry {
    pub port: u16,
    pub protocol: String,
    pub local_addr: String,
    pub pid: u32,
    pub process_name: String,
    pub process_path: Option<String>,
}

pub fn enumerate() -> anyhow::Result<Vec<ListeningEntry>> {
    let af = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let pf = ProtocolFlags::TCP | ProtocolFlags::UDP;
    let sockets = get_sockets_info(af, pf)?;

    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let mut proc_cache: HashMap<u32, (String, Option<String>)> = HashMap::new();
    for (pid, proc) in sys.processes() {
        let name = proc.name().to_string_lossy().to_string();
        let path = proc.exe().map(|p| p.to_string_lossy().to_string());
        proc_cache.insert(pid.as_u32(), (name, path));
    }

    let mut out = Vec::new();
    for s in sockets {
        match s.protocol_socket_info {
            ProtocolSocketInfo::Tcp(tcp) => {
                if tcp.state != TcpState::Listen {
                    continue;
                }
                let pid = s.associated_pids.first().copied().unwrap_or(0);
                let (name, path) = proc_cache
                    .get(&pid)
                    .cloned()
                    .unwrap_or_else(|| ("<unknown>".to_string(), None));
                out.push(ListeningEntry {
                    port: tcp.local_port,
                    protocol: "TCP".to_string(),
                    local_addr: tcp.local_addr.to_string(),
                    pid,
                    process_name: name,
                    process_path: path,
                });
            }
            ProtocolSocketInfo::Udp(udp) => {
                let pid = s.associated_pids.first().copied().unwrap_or(0);
                let (name, path) = proc_cache
                    .get(&pid)
                    .cloned()
                    .unwrap_or_else(|| ("<unknown>".to_string(), None));
                out.push(ListeningEntry {
                    port: udp.local_port,
                    protocol: "UDP".to_string(),
                    local_addr: udp.local_addr.to_string(),
                    pid,
                    process_name: name,
                    process_path: path,
                });
            }
        }
    }
    out.sort_by_key(|e| (e.port, e.protocol.clone()));
    Ok(out)
}
