use axum::{Router, middleware, routing::{get, post}};
use ::config::{Config, Environment, File};
use tokio::{net::TcpListener, sync::mpsc};
use tower_http::trace::TraceLayer;
use scribble::{AppState, config::ScribbleConfig, git, micropub::{self, storage::job::{JobFn, JobQueue}}, path_pattern::PathPattern};
use tracing::{debug, error, info};
use validator::Validate;
use std::{error::Error, process::exit, sync::Arc};


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  tracing_subscriber::fmt::init();

  debug!("loading configuration...");
  let config: ScribbleConfig = Config::builder()
    .add_source(File::with_name("config").required(false))
    .add_source(Environment::default().separator("__"))
    .build()?
    .try_deserialize()?;

  debug!("validating configuration...");
  match config.validate() {
    Ok(_) => (),
    Err(e) => {
      error!("Failed to validate configuration: {e}");
      exit(1);
    }
  }

  let binding = config.server.binding.to_string();

  debug!("creating job queue channel...");
  let (job_tx, mut job_rx) = mpsc::channel::<JobFn>(256);

  debug!("creating app state...");
  let path_pattern = PathPattern::new(&config.micropub.content.path_pattern)?;
  let state = Arc::new(AppState {
    config,
    path_pattern,
    reqwest: reqwest::ClientBuilder::new().build()?,
    job_queue: Arc::new(JobQueue::new(job_tx))
  });

  debug!("starting job queue...");
  tokio::spawn(async move {
    while let Some(job) = job_rx.recv().await {
      if let Err(e) = job().await {
        error!("job failed: {e}")
      }
    }
  });

  debug!("checking git connection...");
  git::try_connect_repo(&state)?;

  debug!("setting up axum routes...");
  let micropub = Router::new()
    .route("/", get(micropub::get::handle).post(micropub::post::handle))
    .route("/media", post(micropub::post::handle_media))
    .layer(middleware::from_fn_with_state(state.clone(), micropub::auth::authorize));

  let app = Router::new()
    .nest("/micropub", micropub)
    .layer(TraceLayer::new_for_http())
    .with_state(state);

  debug!("binding tcp listener...");
  let listener = TcpListener::bind(&binding)
    .await
    .expect("Failed to bind TCP listener");

  info!("Scribble is listening on {binding}");

  let _ = axum::serve(listener, app).await;

  Ok(())
}
