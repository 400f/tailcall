use anyhow::{anyhow, Result};

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

async fn process_resources_metrics() -> Result<()> {
    let meter = opentelemetry::global::meter("process-resources");

    opentelemetry_system_metrics::init_process_observer(meter)
        .await
        .map_err(|err| anyhow!(err))
}

pub async fn init_metrics(runtime: &TargetRuntime) -> Result<()> {
    cache_metrics(runtime)?;
    process_resources_metrics().await?;

    Ok(())
}
