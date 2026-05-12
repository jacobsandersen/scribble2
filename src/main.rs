use async_tempfile::TempDir;
use axum::{Router, middleware, routing::{get, post}};
use ::config::{Config, Environment, File};
use tokio::{net::TcpListener, sync::mpsc};
use tower_http::trace::TraceLayer;
use scribble::{AppState, config::ScribbleConfig, git, micropub::{self, storage::job::{JobFn, JobQueue}}, path_pattern::PathPattern, telemetry};
use tracing::{error, info, warn};
use validator::Validate;
use std::{error::Error, process::exit, sync::Arc};


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  info!("loading configuration...");
  let config: Arc<ScribbleConfig> = Arc::new(Config::builder()
    .add_source(File::with_name("config").required(false))
    .add_source(Environment::default().separator("__"))
    .build()?
    .try_deserialize()?);

  info!("validating configuration...");
  match config.validate() {
    Ok(_) => (),
    Err(e) => {
      error!("Failed to validate configuration: {e}");
      exit(1);
    }
  }

  let binding = config.server.binding.to_string();

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
    job_queue
  });

  info!("starting job queue...");
  let job_queue_thrd = std::thread::Builder::new().name("job_queue".to_string());
  let job_queue_handle = job_queue_thrd.spawn(move || {
    let runtime = tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
      .unwrap();

    runtime.block_on(async move {
      info!("cloning git repository...");
      let repo_path = TempDir::new().await
        .unwrap_or_else(|e| panic!("failed to create temporary directory for git repository: {e}"));

      let git_config = &config.micropub.content.git;

      let repository = git::clone_repo(&git_config, &repo_path)
        .unwrap_or_else(|e| panic!("failed to clone repo: {e}"));

      while let Some(job) = job_rx.recv().await {
        git::update_repo(&git_config, &repository).await.unwrap_or_else(|e| {
          panic!("failed to reset git repo for job: {e}");
        });

        job(&repository).await.unwrap_or_else(|e| {
          panic!("job failed: {e}");
        });
      }
    });
  }).expect("failed to start job thread");

  info!("starting job queue watchdog...");
  tokio::spawn(async move {
    let job_queue_result = tokio::task::spawn_blocking(move || job_queue_handle.join()).await;

    match job_queue_result {
      Ok(Ok(())) => warn!("job queue thread exited unexpectedly"),
      Ok(Err(_)) => error!("job queue thread panicked"),
      Err(_) => error!("job queue watcher task failed")
    }

    std::process::exit(1);
  });

  info!("setting up axum routes...");
  let micropub = Router::new()
    .route("/", get(micropub::get::handle).post(micropub::post::handle))
    .route("/media", post(micropub::post::handle_media))
    .layer(middleware::from_fn_with_state(state.clone(), micropub::auth::authorize));

  let app = Router::new()
    .nest("/micropub", micropub)
    .layer(TraceLayer::new_for_http())
    .with_state(state);

  info!("binding tcp listener...");
  let listener = TcpListener::bind(&binding)
    .await
    .expect("Failed to bind TCP listener");

  info!("Scribble is listening on {binding}");

  let _ = axum::serve(listener, app).await;

  info!("Scribble is shutting down...");

  if let Some((tracer, logger)) = telemetry {
    info!("Shutting down tracer...");
    let _ = tracer.shutdown();

    info!("Shutting down logger...");
    let _ = logger.shutdown();
  }

  info!("Goodbye!");
  
  Ok(())
}
