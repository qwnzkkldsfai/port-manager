use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

const RAW: &str = include_str!("../data/static_ports.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StaticEntry {
    pub port: u16,
    pub name: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
struct Raw {
    iana: Vec<StaticEntry>,
    tools: Vec<StaticEntry>,
}

pub struct StaticTables {
    pub iana: HashMap<u16, StaticEntry>,
    pub tools: HashMap<u16, StaticEntry>,
}

pub static TABLES: Lazy<StaticTables> = Lazy::new(|| {
    let parsed: Raw = serde_json::from_str(RAW).expect("invalid static_ports.json");
    let iana = parsed.iana.into_iter().map(|e| (e.port, e)).collect();
    let tools = parsed.tools.into_iter().map(|e| (e.port, e)).collect();
    StaticTables { iana, tools }
});

pub fn iana(port: u16) -> Option<&'static StaticEntry> {
    TABLES.iana.get(&port)
}

pub fn tool(port: u16) -> Option<&'static StaticEntry> {
    TABLES.tools.get(&port)
}

pub fn all_iana_ports() -> Vec<u16> {
    TABLES.iana.keys().copied().collect()
}

pub fn all_tool_ports() -> Vec<u16> {
    TABLES.tools.keys().copied().collect()
}
