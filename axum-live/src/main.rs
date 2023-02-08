use std::{
    net::SocketAddr,
    sync::{atomic::AtomicUsize, Arc, RwLock}, io,
};

use axum::{
    async_trait,
    extract::{Extension, FromRequestParts, Json, TypedHeader},
    headers::{authorization::Bearer, Authorization},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, get_service},
    Router, Server,
};
use jsonwebtoken as jwt;
use serde::{Deserialize, Serialize};
use tower_http::{trace::TraceLayer, services::{ServeDir, ServeFile}};

const SECRET: &[u8] = b"guessit";

static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

fn get_next_id() -> usize {
    NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todo {
    pub id: usize,
    pub user_id: usize,
    pub title: String,
    pub completed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTodo {
    pub title: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct LoginResponse {
    token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    id: usize,
    name: String,
    exp: u64,
}

#[async_trait]
impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = HttpError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let TypedHeader(Authorization(bearer)) =
            TypedHeader::<Authorization<Bearer>>::from_request_parts(parts, state)
                .await
                .map_err(|_| HttpError::Auth)?;

        let key = jwt::DecodingKey::from_secret(SECRET);
        jwt::decode::<Claims>(bearer.token(), &key, &jwt::Validation::default())
            .map(|data| data.claims)
            .map_err(|_| HttpError::Auth)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub enum HttpError {
    Auth,
    Internal,
}

impl IntoResponse for HttpError {
    fn into_response(self) -> Response {
        let (code, msg) = match self {
            HttpError::Auth => (StatusCode::UNAUTHORIZED, "Unauthorized"),
            HttpError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error"),
        };

        (code, msg).into_response()
    }
}

#[derive(Debug, Default, Clone)]
struct TodoStore {
    items: Arc<RwLock<Vec<Todo>>>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let store = TodoStore::default();

    let serve_dir = ServeDir::new("www/build").not_found_service(ServeFile::new("www/build/index.html"));
    let serve_dir = get_service(serve_dir).handle_error(handle_error);

    let app = Router::new()
        .route(
            "/todos",
            get(todos_handler)
                .post(create_todos_handler)
                .layer(Extension(store)),
        )
        .route("/login", post(login_handler))
        .nest_service("/", serve_dir.clone())
        .fallback_service(serve_dir)
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([127, 0, 0, 1], 8000));
    println!("Listening on http://{}", addr);

    Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn todos_handler(
    claims: Claims,
    Extension(store): Extension<TodoStore>,
) -> Result<Json<Vec<Todo>>, HttpError> {
    let user_id = claims.id;
    match store.items.read() {
        Ok(items) => Ok(Json(
            items
                .iter()
                .filter(|todo| todo.user_id == user_id)
                .cloned()
                .collect(),
        )),
        Err(_) => Err(HttpError::Internal),
    }
}

// eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJpZCI6MSwibmFtZSI6ImppeXVhbjAxMjVAMTI2LmNvbSIsImV4cCI6MTY3NzA2Njc5Mn0.uYcVo5_kphAYi5McO2CvTah9wt6VBP451GzUvTlsau8
async fn create_todos_handler(
    claims: Claims,
    Extension(store): Extension<TodoStore>,
    Json(todo): Json<CreateTodo>,
) -> Result<StatusCode, HttpError> {
    match store.items.write() {
        Ok(mut items) => {
            items.push(Todo {
                id: get_next_id(),
                user_id: claims.id,
                title: todo.title,
                completed: false,
            });
            Ok(StatusCode::CREATED)
        }
        Err(_) => Err(HttpError::Internal),
    }
}

async fn login_handler(Json(_login): Json<LoginRequest>) -> Json<LoginResponse> {
    // skip login info validation
    let claims = Claims {
        id: 1,
        name: "jiyuan0125@126.com".to_string(),
        exp: get_epoch() + 14 * 24 * 60 * 60,
    };
    let key = jwt::EncodingKey::from_secret(SECRET);
    let token = jwt::encode(&jwt::Header::default(), &claims, &key).unwrap();
    Json(LoginResponse { token })
}

fn get_epoch() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

async fn handle_error(_err: io::Error) -> impl IntoResponse {
    (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong...")
}

