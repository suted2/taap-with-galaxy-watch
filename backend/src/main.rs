//! taap QR 백엔드: 워치가 GET /qr 하면 cardSerialNumber 를 반환한다.
//!
//! 내부 흐름: 인계받은 refresh_token 으로 id_token 갱신 → court QR API 조회.
//! - court API 는 access_token 이 아니라 id_token(roles 포함)을 Bearer 로 요구한다.
//! - refresh 는 rotation 이라 매 갱신마다 새 refresh_token 을 파일에 저장한다.
//! - 앱과 refresh 체인을 공유하므로(인계 방식) 앱을 동시에 쓰면 서로 무효화된다. 워치 전용으로 쓸 것.
//!
//! 실행: TAAP_REFRESH_FILE=/path/refresh.txt PORT=8787 cargo run
use axum::{extract::State, http::StatusCode, routing::get, Json, Router};
use std::sync::Arc;
use tokio::sync::Mutex;

const BASE: &str = "https://taapspace.kr";
const CLIENT_ID: &str = "HNXsnajdyjwNWnPvAAYD8javgXTUuq-JuAgfUNcFudg";

#[derive(serde::Deserialize)]
struct TokenResp {
    id_token: String,
    refresh_token: String,
}

#[derive(serde::Deserialize)]
struct QrResp {
    data: QrData,
}
#[derive(serde::Deserialize)]
struct QrData {
    qr: Qr,
}
#[derive(serde::Deserialize, serde::Serialize, Clone)]
struct Qr {
    #[serde(rename = "userId")]
    user_id: i64,
    #[serde(rename = "cardSerialNumber")]
    card_serial_number: String,
}

struct AppState {
    http: reqwest::Client,
    refresh_file: String,
    // refresh_token 파일 읽기/쓰기(rotation)를 직렬화. ponytail: 워치 1대라 전역 락으로 충분.
    lock: Mutex<()>,
}

async fn fetch_qr(st: &AppState) -> Result<Qr, String> {
    let _guard = st.lock.lock().await;

    let refresh_token = std::fs::read_to_string(&st.refresh_file)
        .map_err(|e| format!("refresh_token 읽기 실패: {e}"))?
        .trim()
        .to_string();

    // 1) refresh → id_token (+ 새 refresh_token)
    let tok: TokenResp = st
        .http
        .post(format!("{BASE}/api/court-auth/oauth/token"))
        .form(&[
            ("client_id", CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh_token),
        ])
        .send()
        .await
        .map_err(|e| format!("token 요청 실패: {e}"))?
        .error_for_status()
        .map_err(|e| format!("token 갱신 거부(refresh 만료?): {e}"))?
        .json()
        .await
        .map_err(|e| format!("token 응답 파싱 실패: {e}"))?;

    // rotation: 새 refresh_token 즉시 저장 (안 하면 다음 요청에서 무효)
    std::fs::write(&st.refresh_file, &tok.refresh_token)
        .map_err(|e| format!("refresh_token 저장 실패: {e}"))?;

    // 2) id_token 으로 QR 조회
    let qr: QrResp = st
        .http
        .get(format!("{BASE}/api/court/ac/access/qr"))
        .bearer_auth(&tok.id_token)
        .header("accept", "application/json, text/plain, */*")
        .send()
        .await
        .map_err(|e| format!("QR 요청 실패: {e}"))?
        .error_for_status()
        .map_err(|e| format!("QR 거부: {e}"))?
        .json()
        .await
        .map_err(|e| format!("QR 응답 파싱 실패: {e}"))?;

    Ok(qr.data.qr)
}

async fn qr_handler(State(st): State<Arc<AppState>>) -> Result<Json<Qr>, (StatusCode, String)> {
    fetch_qr(&st)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::BAD_GATEWAY, e))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let refresh_file =
        std::env::var("TAAP_REFRESH_FILE").unwrap_or_else(|_| "taap_refresh_token.txt".into());

    // 저장 경로의 부모 디렉토리 보장 (disk mount path 등)
    if let Some(parent) = std::path::Path::new(&refresh_file).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    // 최초 부트스트랩: refresh 파일이 없으면 env(TAAP_REFRESH_TOKEN)로 초기화한다.
    // 이후 rotation 값은 파일(=persistent disk)에 저장되어 재시작에도 유지된다.
    if !std::path::Path::new(&refresh_file).exists() {
        if let Ok(seed) = std::env::var("TAAP_REFRESH_TOKEN") {
            std::fs::write(&refresh_file, seed.trim())?;
            eprintln!("[init] refresh_token 파일을 env 로 부트스트랩: {refresh_file}");
        }
    }

    let st = Arc::new(AppState {
        http: reqwest::Client::builder().user_agent("okhttp/4.9.2").build()?,
        refresh_file,
        lock: Mutex::new(()),
    });

    let app = Router::new()
        .route("/qr", get(qr_handler))
        .route("/health", get(|| async { "ok" }))
        .with_state(st);

    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8787);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    println!("taap-qr 서버: http://0.0.0.0:{port}/qr");
    axum::serve(listener, app).await?;
    Ok(())
}
