use axum::{Router, middleware, routing::get};
use ::config::{Config, Environment, File};
use tokio::{net::TcpListener, sync::mpsc};
use tower_http::trace::TraceLayer;
use scribble::{AppState, config::ScribbleConfig, git, micropub};
use tracing::{debug, error, info};
use validator::Validate;
use std::{error::Error, process::exit, sync::Arc};


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  tracing_subscriber::fmt::init();

  debug!("loading configuration...");
  let config: ScribbleConfig = Config::builder()
    .add_source(File::with_name("config"))
    .add_source(Environment::default())
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

  debug!("creating mpsc channel...");
  let (tx, mut rx) = mpsc::channel(32);

  debug!("creating app state...");
  let state = Arc::new(AppState {
    config,
    reqwest: reqwest::ClientBuilder::new().build()?,
    writer_tx: tx
  });

  debug!("checking git connection...");
  git::try_connect_repo(&state)?;

  debug!("starting writer job queue...");
  tokio::spawn(async move {
    while let Some(job) = rx.recv().await {
      let result = micropub::storage::store_object(&job).await;
      let _ = job.respond_to.send(result);
    }
  });

  debug!("setting up axum routes...");
  let micropub = Router::new()
    .route("/", get(micropub::get::handle).post(micropub::post::handle))
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
