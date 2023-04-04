use anyhow::Result;
use axum::{
	body::{self, Bytes, Empty, Full},
	extract::{Multipart, Path},
	http::{header, HeaderMap, HeaderValue, Request, StatusCode},
	middleware::{self, Next},
	response::{IntoResponse, Response},
	routing::{get, post},
	Extension, Router,
};
use dotenv::dotenv;
use log::{error, info};
use rand::distributions::{Alphanumeric, DistString};
use simple_logger::SimpleLogger;
use std::{
	collections::HashSet,
	env,
	fs::{read, write},
	net::SocketAddr,
	path::Path as osPath,
};

const DEFAULT_PORT: &str = "8080";
const DEFAULT_MEDIA_DIRECTORY: &str = "www/media";

async fn check_auth<B>(
	headers: HeaderMap,
	Extension(tokens): Extension<HashSet<String>>,
	request: Request<B>,
	next: Next<B>,
) -> Result<Response, StatusCode> {
	// check for auth header
	match headers.get("api_key") {
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

fn generate_file_name() -> String {
	Alphanumeric.sample_string(&mut rand::thread_rng(), 10)
}

struct File {
	name: String,
	bytes: Bytes,
}

impl File {
	fn get_ext(&self) -> &str {
		osPath::new(&self.name)
			.extension()
			.unwrap_or_default()
			.to_str()
			.unwrap_or_default()
	}
}

async fn upload(mut multipart: Multipart) -> (StatusCode, String) {
	let mut file = None;

	while let Some(field) = multipart.next_field().await.unwrap() {
		if field.name().unwrap_or_default() == "file" {
			let name = field.file_name().unwrap_or_default().to_string();
			let bytes = field.bytes().await.unwrap();

			file = Some(File { name, bytes });

			break;
		}
	}

	match file {
		Some(file) => {
			let file_name = format!(
				"{}.{}",
				generate_file_name().to_ascii_lowercase(),
				file.get_ext()
			);
			let file_path = osPath::new(DEFAULT_MEDIA_DIRECTORY).join(file_name.clone());
			write(file_path.as_os_str(), file.bytes).unwrap();

			info!("uploading file {:?}", file_path);
			(StatusCode::ACCEPTED, file_name)
		}
		None => (StatusCode::BAD_REQUEST, "no file found".to_string()),
	}
}

async fn serve_media(Path(path): Path<String>) -> impl IntoResponse {
	let path = path.trim_start_matches('/');
	let mime_type = mime_guess::from_path(path).first_or_text_plain();

	let file_path = osPath::new(DEFAULT_MEDIA_DIRECTORY).join(path);

	let file = read(file_path.as_os_str());
	info!("attempting to serve file {:?}", file_path);

	match file {
		Err(_) => Response::builder()
			.status(StatusCode::NOT_FOUND)
			.body(body::boxed(Empty::new()))
			.unwrap(),
		Ok(contents) => Response::builder()
			.status(StatusCode::OK)
			.header(
				header::CONTENT_TYPE,
				HeaderValue::from_str(mime_type.as_ref()).unwrap(),
			)
			.body(body::boxed(Full::from(contents)))
			.unwrap(),
	}
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

	let content = Router::new()
		// upload, delete, edit media
		.route("/", post(upload))
		.layer(middleware::from_fn(check_auth))
		.layer(Extension(tokens))
		// serve media
		.route("/:path", get(serve_media));

	let app = Router::new().nest("", content);

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
