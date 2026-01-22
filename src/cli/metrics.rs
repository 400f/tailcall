use anyhow::Result;

use crate::core::runtime::TargetRuntime;

fn cache_metrics(runtime: &TargetRuntime) -> Result<()> {
    let meter = opentelemetry::global::meter("cache");
    let cache = runtime.cache.clone();
    let _counter = meter
        .f64_observable_gauge("cache.hit_rate")
        .with_description("Cache hit rate ratio")
        .with_callback(move |observer| {
            if let Some(hit_rate) = cache.hit_rate() {
                observer.observe(hit_rate, &[]);
            }
        })
        .build();

    Ok(())
}

fn process_resources_metrics() {
    let meter = opentelemetry::global::meter("process-resources");

    // init_process_observer runs an infinite loop to collect system metrics,
    // so it must be spawned as a background task
    tokio::spawn(async move {
        if let Err(err) = opentelemetry_system_metrics::init_process_observer(meter).await {
            tracing::warn!("process_resources_metrics failed: {}", err);
        }
    });
}

pub async fn init_metrics(runtime: &TargetRuntime) -> Result<()> {
    cache_metrics(runtime)?;
    process_resources_metrics();

    Ok(())
}
