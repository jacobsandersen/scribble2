use axum::{Router, middleware, routing::get};
use ::config::{Config, Environment, File};
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use scribble::{AppState, config::ScribbleConfig, micropub};
use tracing::{error, info};
use validator::Validate;
use std::{error::Error, process::exit, sync::Arc};


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  tracing_subscriber::fmt::init();

  let config: ScribbleConfig = Config::builder()
    .add_source(File::with_name("config"))
    .add_source(Environment::default())
    .build()?
    .try_deserialize()?;

  match config.validate() {
    Ok(_) => (),
    Err(e) => {
      error!("Failed to validate configuration: {e}");
      exit(1);
    }
  }

  let binding = config.server.binding.to_string();

  let state = Arc::new(AppState {
    config,
    reqwest: reqwest::ClientBuilder::new().build()?
  });

  let micropub = Router::new()
    .route("/", get(micropub::get::handle).post(micropub::post::handle))
    .layer(middleware::from_fn_with_state(state.clone(), micropub::auth::authorize));

  let app = Router::new()
    .nest("/micropub", micropub)
    .layer(TraceLayer::new_for_http())
    .with_state(state);

  let listener = TcpListener::bind(&binding)
    .await
    .expect("Failed to bind TCP listener");

  info!("Scribble is listening on {binding}");

  let _ = axum::serve(listener, app).await;

  Ok(())
}
