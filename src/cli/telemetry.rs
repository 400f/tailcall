use once_cell::sync::Lazy;
use opentelemetry::propagation::TextMapCompositePropagator;
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::{global, KeyValue};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{
    LogExporter, MetricExporter, SpanExporter, WithExportConfig, WithTonicConfig,
};
use opentelemetry_sdk::logs::{SdkLogger, SdkLoggerProvider};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_sdk::propagation::{BaggagePropagator, TraceContextPropagator};
use opentelemetry_sdk::trace::{SdkTracerProvider, Tracer};
use opentelemetry_sdk::Resource;
use tonic::metadata::MetadataMap;
use tracing::level_filters::LevelFilter;
use tracing::Subscriber;
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_subscriber::filter::dynamic_filter_fn;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{Layer, Registry};

use super::metrics::init_metrics;
use crate::core::blueprint::telemetry::{Telemetry, TelemetryExporter};
use crate::core::runtime::TargetRuntime;
use crate::core::tracing::{
    default_tracing, default_tracing_tailcall, get_log_level, tailcall_filter_target,
};

static RESOURCE: Lazy<Resource> = Lazy::new(|| {
    Resource::builder()
        .with_service_name("tailcall")
        .with_attribute(KeyValue::new(
            opentelemetry_semantic_conventions::attribute::SERVICE_VERSION,
            option_env!("APP_VERSION").unwrap_or("dev"),
        ))
        .build()
});

fn set_trace_provider(
    exporter: &TelemetryExporter,
) -> anyhow::Result<Option<OpenTelemetryLayer<Registry, Tracer>>> {
    let provider = match exporter {
        TelemetryExporter::Stdout(_config) => {
            let exporter = opentelemetry_stdout::SpanExporter::default();
            SdkTracerProvider::builder()
                .with_batch_exporter(exporter)
                .with_resource(RESOURCE.clone())
                .build()
        }
        TelemetryExporter::Otlp(config) => {
            let exporter = SpanExporter::builder()
                .with_tonic()
                .with_endpoint(config.url.as_str())
                .with_metadata(MetadataMap::from_headers(config.headers.clone()))
                .build()?;
            SdkTracerProvider::builder()
                .with_batch_exporter(exporter)
                .with_resource(RESOURCE.clone())
                .build()
        }
        // Prometheus works only with metrics
        TelemetryExporter::Prometheus(_) => return Ok(None),
        TelemetryExporter::Apollo(_) => return Ok(None),
    };
    let tracer = provider.tracer("tracing");
    let telemetry = tracing_opentelemetry::layer()
        .with_location(false)
        .with_threads(false)
        .with_tracer(tracer);

    global::set_tracer_provider(provider);

    Ok(Some(telemetry))
}

fn set_logger_provider(
    exporter: &TelemetryExporter,
) -> anyhow::Result<Option<OpenTelemetryTracingBridge<SdkLoggerProvider, SdkLogger>>> {
    let provider = match exporter {
        TelemetryExporter::Stdout(_config) => {
            let exporter = opentelemetry_stdout::LogExporter::default();
            SdkLoggerProvider::builder()
                .with_batch_exporter(exporter)
                .with_resource(RESOURCE.clone())
                .build()
        }
        TelemetryExporter::Otlp(config) => {
            let exporter = LogExporter::builder()
                .with_tonic()
                .with_endpoint(config.url.as_str())
                .with_metadata(MetadataMap::from_headers(config.headers.clone()))
                .build()?;
            SdkLoggerProvider::builder()
                .with_batch_exporter(exporter)
                .with_resource(RESOURCE.clone())
                .build()
        }
        // Prometheus works only with metrics
        TelemetryExporter::Prometheus(_) => return Ok(None),
        TelemetryExporter::Apollo(_) => return Ok(None),
    };

    let otel_tracing_appender = OpenTelemetryTracingBridge::new(&provider);

    Ok(Some(otel_tracing_appender))
}

fn set_meter_provider(exporter: &TelemetryExporter) -> anyhow::Result<()> {
    let provider = match exporter {
        TelemetryExporter::Stdout(_config) => {
            let exporter = opentelemetry_stdout::MetricExporter::default();
            SdkMeterProvider::builder()
                .with_periodic_exporter(exporter)
                .with_resource(RESOURCE.clone())
                .build()
        }
        TelemetryExporter::Otlp(config) => {
            let exporter = MetricExporter::builder()
                .with_tonic()
                .with_endpoint(config.url.as_str())
                .with_metadata(MetadataMap::from_headers(config.headers.clone()))
                .build()?;
            SdkMeterProvider::builder()
                .with_periodic_exporter(exporter)
                .with_resource(RESOURCE.clone())
                .build()
        }
        TelemetryExporter::Prometheus(_) => {
            let exporter = opentelemetry_prometheus::exporter()
                .with_registry(prometheus::default_registry().clone())
                .build()?;
            SdkMeterProvider::builder()
                .with_resource(RESOURCE.clone())
                .with_reader(exporter)
                .build()
        }
        _ => return Ok(()),
    };

    global::set_meter_provider(provider);

    Ok(())
}

fn set_tracing_subscriber(subscriber: impl Subscriber + Send + Sync) {
    // ignore errors since there is only one possible error when the global
    // subscriber is already set. The init is called multiple times in the same
    // process inside tests, so we want to ignore if it is already set
    let _ = tracing::subscriber::set_global_default(subscriber);
}

pub async fn init_opentelemetry(config: Telemetry, runtime: &TargetRuntime) -> anyhow::Result<()> {
    if let Some(export) = &config.export {
        let trace_layer = set_trace_provider(export)?;
        let log_layer = set_logger_provider(export)?;
        set_meter_provider(export)?;

        global::set_text_map_propagator(TextMapCompositePropagator::new(vec![
            Box::new(TraceContextPropagator::new()),
            Box::new(BaggagePropagator::new()),
        ]));

        let subscriber = tracing_subscriber::registry()
            .with(trace_layer)
            .with(default_tracing())
            .with(
                log_layer.with_filter(dynamic_filter_fn(|_metatada, context| {
                    // ignore logs that are generated inside tracing::Span since they will be logged
                    // anyway with tracer_provider and log here only the events without associated
                    // span
                    context.lookup_current().is_none()
                })),
            )
            .with(tailcall_filter_target())
            .with(LevelFilter::from_level(
                get_log_level().unwrap_or(tracing::Level::INFO),
            ));

        init_metrics(runtime).await?;

        set_tracing_subscriber(subscriber);
    } else {
        set_tracing_subscriber(default_tracing_tailcall());
    }

    Ok(())
}
