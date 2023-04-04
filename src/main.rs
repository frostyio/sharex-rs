use anyhow::Result;
use axum::{
	http::{HeaderMap, Request, StatusCode},
	middleware::{self, Next},
	response::Response,
	routing::post,
	Extension, Router,
};
use dotenv::dotenv;
use log::{error, info};
use simple_logger::SimpleLogger;
use std::{collections::HashSet, env, net::SocketAddr};

const DEFAULT_PORT: &str = "8080";

async fn check_auth<B>(
	headers: HeaderMap,
	Extension(tokens): Extension<HashSet<String>>,
	request: Request<B>,
	next: Next<B>,
) -> Result<Response, StatusCode> {
	// check for auth header
	match headers.get("authorization") {
		Some(token_header) => {
			// convert auth token to &str
			let token = match token_header.to_str() {
				Ok(token) => token,
				Err(err) => {
					error!("unable to convert token to &str, {:?}", err);
					return Err(StatusCode::UNAUTHORIZED);
				}
			};

			// check for auth token in tokens
			match tokens.contains(token) {
				true => {
					let response = next.run(request).await;
					Ok(response)
				}
				false => Err(StatusCode::UNAUTHORIZED),
			}
		}
		None => Err(StatusCode::UNAUTHORIZED),
	}
}

async fn upload() -> (StatusCode, &'static str) {
	(StatusCode::ACCEPTED, "hello")
}

fn get_tokens() -> HashSet<String> {
	let tokens_str = env::var("TOKENS").unwrap_or_default();
	tokens_str.split(",").map(|t| t.to_owned()).collect()
}

async fn upload_server() -> Result<()> {
	// create socketaddr
	let port: u16 = env::var("PORT")
		.unwrap_or(DEFAULT_PORT.to_string())
		.parse()?;
	let addr = SocketAddr::from(([127, 0, 0, 1], port));

	// tokens
	let tokens = get_tokens();

	// add routes
	let app = Router::new()
		.route("/", post(upload))
		.layer(middleware::from_fn(check_auth))
		.layer(Extension(tokens));

	// startup server
	info!("running sharex upload server on http://localhost:{}", port);
	axum::Server::bind(&addr)
		.serve(app.into_make_service())
		.await
		.unwrap();

	Ok(())
}

#[tokio::main]
async fn main() {
	SimpleLogger::new()
		.with_level(log::LevelFilter::Info)
		.init()
		.unwrap();
	dotenv().ok();

	match upload_server().await {
		Ok(_) => {}
		Err(err) => panic!("{}", err),
	};
}
