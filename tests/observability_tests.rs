//! The two servers of plan O1 over real sockets (`specs/tasks/observability.md`): the public one
//! answers the health checks and not `/metrics`; the internal one, read by Fly, only `/metrics`.

use just_ingredients::observability::{self, PhotoResult, server};
use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;

async fn get(address: SocketAddr, path: &str) -> anyhow::Result<reqwest::Response> {
    Ok(reqwest::get(format!("http://{address}{path}")).await?)
}

#[tokio::test]
async fn health_is_public_and_metrics_internal() -> anyhow::Result<()> {
    let recorder = PrometheusBuilder::new()
        .set_buckets(&[0.1, 1.0, 10.0])?
        .build_recorder();
    let handle = recorder.handle();
    metrics::with_local_recorder(&recorder, || {
        observability::photo(PhotoResult::Read);
        observability::gemini_usage("gemini-test", 10, 5, Some(42));
        observability::card_shown(std::time::Duration::from_millis(300));
    });
    let addresses = server::start(handle, 0, 0, false, None).await?;
    assert!(addresses.health.ip().is_loopback() && addresses.metrics.ip().is_loopback());

    let metrics = get(addresses.metrics, "/metrics").await?;
    assert_eq!(metrics.status(), 200);
    let body = metrics.text().await?;
    assert!(
        body.contains(r#"ji_photos_total{result="read"} 1"#),
        "{body}"
    );
    assert!(
        body.contains(r#"ji_gemini_cost_micro_usd_total{model="gemini-test"} 42"#),
        "{body}"
    );
    assert!(
        body.contains(r#"ji_card_shown_seconds_bucket{le="1"} 1"#),
        "durations are histograms: {body}"
    );
    assert_eq!(get(addresses.metrics, "/health/live").await?.status(), 404);

    assert_eq!(get(addresses.health, "/health/live").await?.status(), 200);
    assert_eq!(
        get(addresses.health, "/health/ready").await?.status(),
        200,
        "no database nor token to check"
    );
    assert_eq!(
        get(addresses.health, "/metrics").await?.status(),
        404,
        "no public metrics"
    );
    Ok(())
}
