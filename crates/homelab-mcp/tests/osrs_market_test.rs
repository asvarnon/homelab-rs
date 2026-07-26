use homelab_mcp::osrs_market::{OsrsMarketClient, TimeSeriesLookback};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn lookup_uses_the_documented_latest_route_and_returns_the_requested_item() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/latest"))
        .and(query_param("id", "4151"))
        .and(header("User-Agent", "homelab-rs/0.1.0 (OSRS market intelligence; https://github.com/asvarnon/homelab-rs)"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": {
                "4151": { "high": 1_000_000, "highTime": 1_785_084_573, "low": 986_000, "lowTime": 1_785_084_513 }
            }
        })))
        .mount(&server)
        .await;

    let client = OsrsMarketClient::with_base_url(server.uri()).expect("client should build");
    let price = client.lookup(4151).await.expect("lookup should succeed");

    assert_eq!(price.item_id, 4151);
    assert_eq!(price.high, Some(1_000_000));
    assert_eq!(price.low, Some(986_000));
}

#[tokio::test]
async fn mapping_fetches_item_metadata_from_the_bulk_route() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/mapping"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": 4151, "name": "Abyssal whip", "members": true, "limit": 70, "highalch": 72_000, "lowalch": 48_000, "value": 120_000, "examine": "A weapon from the abyss.", "icon": "Abyssal whip.png" }
        ])))
        .mount(&server)
        .await;

    let client = OsrsMarketClient::with_base_url(server.uri()).expect("client should build");
    let mapping = client.mapping().await.expect("mapping should succeed");

    assert_eq!(mapping[0].name, "Abyssal whip");
    assert_eq!(mapping[0].limit, Some(70));
}

#[tokio::test]
async fn activity_fetches_the_bulk_five_minute_snapshot_without_an_item_query() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/5m"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": { "4151": { "avgHighPrice": 1_000_000.5, "highPriceVolume": 5, "avgLowPrice": 986_000, "lowPriceVolume": 8 } }
        })))
        .mount(&server)
        .await;

    let client = OsrsMarketClient::with_base_url(server.uri()).expect("client should build");
    let activity = client.activity_5m().await.expect("activity should succeed");

    assert_eq!(activity[&4151].avg_high_price, Some(1_000_000.5));
    assert_eq!(activity[&4151].high_price_volume, 5);
}

#[tokio::test]
async fn activity_fetches_the_bulk_hourly_snapshot_without_an_item_query() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/1h"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": { "4151": { "avgHighPrice": 1_001_500.25, "highPriceVolume": 150, "avgLowPrice": 985_250.5, "lowPriceVolume": 190 } }
        })))
        .mount(&server)
        .await;

    let client = OsrsMarketClient::with_base_url(server.uri()).expect("client should build");
    let activity = client.activity_1h().await.expect("activity should succeed");

    assert_eq!(activity[&4151].avg_high_price, Some(1_001_500.25));
    assert_eq!(activity[&4151].low_price_volume, 190);
}

#[tokio::test]
async fn history_sends_the_item_and_lookback_query_parameters() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/timeseries"))
        .and(query_param("id", "4151"))
        .and(query_param("lookback", "24h"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [{ "timestamp": 1_785_084_500, "avgHighPrice": 1_000_000, "highPriceVolume": 5, "avgLowPrice": 986_000, "lowPriceVolume": 8 }]
        })))
        .mount(&server)
        .await;

    let client = OsrsMarketClient::with_base_url(server.uri()).expect("client should build");
    let history = client
        .history(4151, TimeSeriesLookback::Hours24)
        .await
        .expect("history should succeed");

    assert_eq!(history[0].timestamp, 1_785_084_500);
}
