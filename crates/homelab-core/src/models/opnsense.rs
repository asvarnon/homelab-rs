use serde::{Deserialize, Serialize};
use std::net::Ipv4Addr;

//generic result response for opnsense api endpoints
#[derive(Debug, Deserialize)]
pub struct SearchResponse<T> {
    pub total: u32,
    #[serde(rename = "rowCount")]
    pub row_count: u32,
    pub current: u32,
    pub rows: Vec<T>, // can be any type. a list of dchp leases, etc. config struct for whichever type you need
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DhcpLease {
    #[serde(rename = "address")]
    pub ip: Ipv4Addr,
    #[serde(rename = "hwaddr")]
    pub mac: String,
    pub hostname: Option<String>,
    #[serde(rename = "if")]
    pub interface: String,
    #[serde(rename = "if_descr")]
    pub interface_desc: String,
    pub expire: u64,
}

//may or may not use below..

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpSuggestion {
    pub suggested_ip: Ipv4Addr,
    pub vlan: u16,
    pub reasoning: String,
}
