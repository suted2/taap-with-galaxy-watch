//! taap QR 재현: refresh_token 으로 access 갱신 → QR 조회 → cardSerialNumber 출력.
//!
//! taap access token 수명은 5분이고 refresh 는 rotation(매번 새 값) 이므로,
//! refresh_token 을 파일에 두고 매 실행마다 갱신된 값으로 덮어쓴다.
//!
//! 실행: TAAP_REFRESH_FILE=/path/to/refresh.txt cargo run
use std::fs;

const BASE: &str = "https://taapspace.kr";
// public client (client_secret 없음). 앱에 하드코딩된 공개 식별자.
const CLIENT_ID: &str = "HNXsnajdyjwNWnPvAAYD8javgXTUuq-JuAgfUNcFudg";

#[derive(serde::Deserialize)]
struct TokenResp {
    // court API 는 access_token 이 아니라 id_token(roles 포함)을 Bearer 로 요구한다.
    id_token: String,
    refresh_token: String,
    expires_in: i64,
}

#[derive(serde::Deserialize)]
struct QrResp {
    data: QrData,
}
#[derive(serde::Deserialize)]
struct QrData {
    qr: Qr,
}
#[derive(serde::Deserialize)]
struct Qr {
    #[serde(rename = "cardSerialNumber")]
    card_serial_number: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let refresh_file =
        std::env::var("TAAP_REFRESH_FILE").unwrap_or_else(|_| "taap_refresh_token.txt".into());
    let refresh_token = fs::read_to_string(&refresh_file)?.trim().to_string();

    let http = reqwest::Client::builder()
        .user_agent("okhttp/4.9.2")
        .cookie_store(true) // oauth/token 이 주는 SESSION 쿠키를 QR 요청에 자동 첨부
        .build()?;

    // 1) refresh_token 으로 access token 갱신
    let tok: TokenResp = http
        .post(format!("{BASE}/api/court-auth/oauth/token"))
        .form(&[
            ("client_id", CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", &refresh_token),
        ])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    eprintln!("[ok] access token 갱신 (expires_in={}s)", tok.expires_in);

    // rotation: 새 refresh_token 을 즉시 저장 (안 하면 다음 실행에서 무효)
    fs::write(&refresh_file, &tok.refresh_token)?;

    // 2) QR 조회
    let qr: QrResp = http
        .get(format!("{BASE}/api/court/ac/access/qr"))
        .bearer_auth(&tok.id_token) // access_token 아님 — court API 는 id_token 을 본다
        .header("accept", "application/json, text/plain, */*")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    // cardSerialNumber 를 stdout 으로 (워치가 이 문자열을 QR 로 렌더)
    println!("{}", qr.data.qr.card_serial_number);
    Ok(())
}
