use ::config::{Config, Environment, File};
use async_tempfile::TempDir;
use axum::{
    Router, http, middleware,
    routing::{get, post},
};
use axum_prometheus::{PrometheusMetricLayer, metrics_exporter_prometheus::PrometheusHandle};
use moka::future::Cache;
use opentelemetry::global;
use opentelemetry_http::HeaderExtractor;
use scribble::{
    AppState,
    config::ScribbleConfig,
    git,
    micropub::{
        self,
        storage::job::{JobFn, JobQueue},
    },
    path_pattern::PathPattern,
    telemetry,
};
use std::{error::Error, process::exit, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::mpsc};
use tower_http::trace::TraceLayer;
use tracing::{error, info, info_span, warn};
use tracing_opentelemetry::OpenTelemetrySpanExt;
use validator::Validate;

fn public_router(state: Arc<AppState>) -> (Router, PrometheusHandle) {
    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

    let micropub = Router::new()
        .route("/", get(micropub::get::handle).post(micropub::post::handle))
        .route("/media", post(micropub::post::handle_media))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            micropub::auth::authorize,
        ));

    let app = Router::new()
        .nest("/micropub", micropub)
        .layer(
            TraceLayer::new_for_http().make_span_with(|req: &http::Request<_>| {
                let parent_cx = global::get_text_map_propagator(|prop| {
                    prop.extract(&HeaderExtractor(req.headers()))
                });

                let ua = req
                    .headers()
                    .get("user-agent")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("unknown");

                let ip = req
                    .headers()
                    .get("x-forwarded-for")
                    .or(req.headers().get("x-real-ip"))
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("unknown");

                let span = info_span!(
                  "http.request",
                  method = %&req.method(),
                  uri = %&req.uri(),
                  user_agent = %ua,
                  client_ip = %ip
                );

                let _ = span.set_parent(parent_cx);
                span
            }),
        )
        .layer(prometheus_layer)
        .with_state(state);

    (app, metric_handle)
}

fn metric_router(handle: PrometheusHandle) -> Router {
    Router::new().route("/metrics", get(|| async move { handle.render() }))
}

async fn serve(router: Router, binding: &str) {
    let listener = TcpListener::bind(&binding)
        .await
        .expect(&format!("failed to bind TCP listener: {binding}"));

    axum::serve(listener, router).await.unwrap();
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    info!("loading configuration...");
    let config: Arc<ScribbleConfig> = Arc::new(
        Config::builder()
            .add_source(File::with_name("config").required(false))
            .add_source(Environment::default().separator("__"))
            .build()?
            .try_deserialize()?,
    );

    info!("validating configuration...");
    match config.validate() {
        Ok(_) => (),
        Err(e) => {
            error!("Failed to validate configuration: {e}");
            exit(1);
        }
    }

    let binding = config.server.binding.to_string();
    let metrics_binding = config.server.metrics_binding.to_string();

    info!("setting up telemetry...");
    let telemetry = telemetry::init_telemetry(&config.monitoring)?;

    info!("creating app state...");
    let path_pattern = PathPattern::new(&config.micropub.content.path_pattern)?;
    let (job_tx, mut job_rx) = mpsc::channel::<JobFn>(256);
    let job_queue = Arc::new(JobQueue::new(job_tx));
    let state = Arc::new(AppState {
        config: config.clone(),
        path_pattern,
        reqwest: reqwest::ClientBuilder::new().build()?,
        job_queue,
        auth_cache: Cache::builder()
            .time_to_live(Duration::from_mins(10))
            .max_capacity(64)
            .build(),
    });

    info!("starting job queue...");
    let job_queue_thrd = std::thread::Builder::new().name("job_queue".to_string());
    let job_queue_handle = job_queue_thrd
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();

            runtime.block_on(async move {
                info!("cloning git repository...");
                let repo_path = TempDir::new().await.unwrap_or_else(|e| {
                    panic!("failed to create temporary directory for git repository: {e}")
                });

                let repository = git::clone_repo(&config.micropub.content.git, &repo_path)
                    .unwrap_or_else(|e| panic!("failed to clone repo: {e}"));

                while let Some(job) = job_rx.recv().await {
                    job(&repository).await.unwrap_or_else(|e| {
                        panic!("job failed: {e}");
                    });
                }
            });
        })
        .expect("failed to start job thread");

    info!("starting job queue watchdog...");
    tokio::spawn(async move {
        let job_queue_result = tokio::task::spawn_blocking(move || job_queue_handle.join()).await;

        match job_queue_result {
            Ok(Ok(())) => warn!("job queue thread exited unexpectedly"),
            Ok(Err(_)) => error!("job queue thread panicked"),
            Err(_) => error!("job queue watcher task failed"),
        }

        std::process::exit(1);
    });

    info!("setting up axum routes...");
    let (public, metric_handle) = public_router(state);
    let metrics = metric_router(metric_handle);

    info!("scribble is listening on {binding} (metrics on {metrics_binding})");

    tokio::join!(serve(public, &binding), serve(metrics, &metrics_binding));

    info!("scribble is shutting down...");

    if let Some((tracer, logger)) = telemetry {
        info!("Shutting down tracer...");
        let _ = tracer.shutdown();

        info!("Shutting down logger...");
        let _ = logger.shutdown();
    }

    info!("Goodbye!");

    Ok(())
}
