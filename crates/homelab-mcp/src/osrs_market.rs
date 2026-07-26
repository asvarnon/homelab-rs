//! Client and domain types for the public OSRS Wiki Prices API v2.
//!
//! This is deliberately separate from `HomelabClient`: the Wiki API is public,
//! has a fixed base URL, and requires a descriptive User-Agent rather than a
//! configured homelab endpoint or authentication.

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const BASE_URL: &str = "https://prices.runescape.wiki/api/v2/osrs";
const USER_AGENT: &str =
    "homelab-rs/0.1.0 (OSRS market intelligence; https://github.com/asvarnon/homelab-rs)";
const MAPPING_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const ACTIVITY_5M_TTL: Duration = Duration::from_secs(5 * 60);
const ACTIVITY_1H_TTL: Duration = Duration::from_secs(60 * 60);
const LATEST_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ItemMapping {
    pub id: u32,
    pub name: String,
    pub members: bool,
    #[serde(rename = "limit")]
    pub limit: Option<u32>,
    #[serde(rename = "highalch")]
    pub high_alch: Option<u32>,
    #[serde(rename = "lowalch")]
    pub low_alch: Option<u32>,
    pub value: Option<u32>,
    pub examine: Option<String>,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LatestPriceResponse {
    data: HashMap<u32, RawLatestPrice>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawLatestPrice {
    high: Option<u64>,
    #[serde(rename = "highTime")]
    high_time: Option<i64>,
    low: Option<u64>,
    #[serde(rename = "lowTime")]
    low_time: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LatestPrice {
    pub item_id: u32,
    pub high: Option<u64>,
    pub high_time: Option<i64>,
    pub low: Option<u64>,
    pub low_time: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct ActivityResponse {
    data: HashMap<u32, ActivitySnapshot>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ActivitySnapshot {
    #[serde(rename = "avgHighPrice")]
    pub avg_high_price: Option<f64>,
    #[serde(rename = "highPriceVolume")]
    pub high_price_volume: u64,
    #[serde(rename = "avgLowPrice")]
    pub avg_low_price: Option<f64>,
    #[serde(rename = "lowPriceVolume")]
    pub low_price_volume: u64,
}

#[derive(Debug, Clone, Deserialize)]
struct HistoryResponse {
    data: Vec<HistoryPoint>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HistoryPoint {
    pub timestamp: i64,
    #[serde(rename = "avgHighPrice")]
    pub avg_high_price: Option<f64>,
    #[serde(rename = "highPriceVolume")]
    pub high_price_volume: u64,
    #[serde(rename = "avgLowPrice")]
    pub avg_low_price: Option<f64>,
    #[serde(rename = "lowPriceVolume")]
    pub low_price_volume: u64,
}

#[derive(Debug, Clone, Copy)]
pub enum TimeSeriesLookback {
    Hours6,
    Hours24,
    Days7,
    Days30,
    Months6,
    Year1,
}

impl TimeSeriesLookback {
    fn as_api_value(self) -> &'static str {
        match self {
            Self::Hours6 => "6h",
            Self::Hours24 => "24h",
            Self::Days7 => "7d",
            Self::Days30 => "30d",
            Self::Months6 => "6m",
            Self::Year1 => "1y",
        }
    }
}

#[derive(Debug, Clone)]
struct Cached<T> {
    value: T,
    fetched_at: Instant,
}

impl<T> Cached<T> {
    fn is_fresh(&self, ttl: Duration) -> bool {
        self.fetched_at.elapsed() < ttl
    }
}

pub struct OsrsMarketClient {
    client: Client,
    base_url: String,
    mapping_cache: Mutex<Option<Cached<Vec<ItemMapping>>>>,
    latest_cache: Mutex<HashMap<u32, Cached<LatestPrice>>>,
    activity_5m_cache: Mutex<Option<Cached<HashMap<u32, ActivitySnapshot>>>>,
    activity_1h_cache: Mutex<Option<Cached<HashMap<u32, ActivitySnapshot>>>>,
}

impl OsrsMarketClient {
    pub fn new() -> Result<Self> {
        Self::with_base_url(BASE_URL)
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Result<Self> {
        let client = Client::builder().user_agent(USER_AGENT).build()?;

        Ok(Self {
            client,
            base_url: base_url.into().trim_end_matches('/').to_string(),
            mapping_cache: Mutex::new(None),
            latest_cache: Mutex::new(HashMap::new()),
            activity_5m_cache: Mutex::new(None),
            activity_1h_cache: Mutex::new(None),
        })
    }

    /// Gets the latest observed low/high trades for an item. Values may come
    /// from different trades and timestamps; they are leads, not fill quotes.
    pub async fn lookup(&self, item_id: u32) -> Result<LatestPrice> {
        if let Some(cached) = self
            .latest_cache
            .lock()
            .expect("latest-price cache mutex poisoned")
            .get(&item_id)
        {
            if cached.is_fresh(LATEST_TTL) {
                return Ok(cached.value.clone());
            }
        }

        let response: LatestPriceResponse = self
            .client
            .get(format!("{}/latest", self.base_url))
            .query(&[("id", item_id)])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let raw = response
            .data
            .get(&item_id)
            .ok_or_else(|| anyhow!("OSRS Wiki returned no latest price for item {item_id}"))?;
        let price = LatestPrice {
            item_id,
            high: raw.high,
            high_time: raw.high_time,
            low: raw.low,
            low_time: raw.low_time,
        };

        self.latest_cache
            .lock()
            .expect("latest-price cache mutex poisoned")
            .insert(
                item_id,
                Cached {
                    value: price.clone(),
                    fetched_at: Instant::now(),
                },
            );
        Ok(price)
    }

    /// Gets item metadata from the bulk mapping endpoint, cached for 24 hours.
    pub async fn mapping(&self) -> Result<Vec<ItemMapping>> {
        if let Some(cached) = self
            .mapping_cache
            .lock()
            .expect("mapping cache mutex poisoned")
            .as_ref()
        {
            if cached.is_fresh(MAPPING_TTL) {
                return Ok(cached.value.clone());
            }
        }

        let mapping = self
            .client
            .get(format!("{}/mapping", self.base_url))
            .send()
            .await?
            .error_for_status()?
            .json::<Vec<ItemMapping>>()
            .await?;
        *self
            .mapping_cache
            .lock()
            .expect("mapping cache mutex poisoned") = Some(Cached {
            value: mapping.clone(),
            fetched_at: Instant::now(),
        });
        Ok(mapping)
    }

    /// Gets the bulk five-minute activity snapshot. The API does not filter
    /// this endpoint by item ID, so callers filter the returned map locally.
    pub async fn activity_5m(&self) -> Result<HashMap<u32, ActivitySnapshot>> {
        if let Some(cached) = self
            .activity_5m_cache
            .lock()
            .expect("five-minute activity cache mutex poisoned")
            .as_ref()
        {
            if cached.is_fresh(ACTIVITY_5M_TTL) {
                return Ok(cached.value.clone());
            }
        }

        let response: ActivityResponse = self
            .client
            .get(format!("{}/5m", self.base_url))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        *self
            .activity_5m_cache
            .lock()
            .expect("five-minute activity cache mutex poisoned") = Some(Cached {
            value: response.data.clone(),
            fetched_at: Instant::now(),
        });
        Ok(response.data)
    }

    /// Gets the bulk hourly activity snapshot for sustained-liquidity checks.
    pub async fn activity_1h(&self) -> Result<HashMap<u32, ActivitySnapshot>> {
        if let Some(cached) = self
            .activity_1h_cache
            .lock()
            .expect("hourly activity cache mutex poisoned")
            .as_ref()
        {
            if cached.is_fresh(ACTIVITY_1H_TTL) {
                return Ok(cached.value.clone());
            }
        }

        let response: ActivityResponse = self
            .client
            .get(format!("{}/1h", self.base_url))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        *self
            .activity_1h_cache
            .lock()
            .expect("hourly activity cache mutex poisoned") = Some(Cached {
            value: response.data.clone(),
            fetched_at: Instant::now(),
        });
        Ok(response.data)
    }

    pub async fn history(
        &self,
        item_id: u32,
        lookback: TimeSeriesLookback,
    ) -> Result<Vec<HistoryPoint>> {
        let response: HistoryResponse = self
            .client
            .get(format!("{}/timeseries", self.base_url))
            .query(&[
                ("id", item_id.to_string()),
                ("lookback", lookback.as_api_value().to_string()),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(response.data)
    }
}
