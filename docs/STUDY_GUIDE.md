# taap-with-galaxy-watch 학습 가이드

이 프로젝트가 어떻게 돌아가는지, 각 단계에서 어떤 개념이 쓰였고 **왜** 그렇게 했는지
정리한 문서. "남의 앱이 서버에서 받아오는 QR을, 내 워치로 재현해 띄운다"는 목표를
처음부터 끝까지 따라간다.

## 0. 큰 그림

```
[갤럭시 워치 앱]  --HTTP GET /qr-->  [Render 백엔드(Rust)]  --HTTPS-->  [taap 서버]
   버튼 탭                              refresh→id_token→QR조회         taapspace.kr
   QR 렌더 (ZXing)                      cardSerialNumber 반환
```

핵심 질문 3개와 답:
1. **taap이 서버에서 뭘 어떻게 받아오나?** → 트래픽을 가로채서(MITM) API를 분석
2. **그 API를 내가 어떻게 다시 호출하나?** → OAuth2 토큰으로 인증, Rust로 재현
3. **워치에서 어떻게 띄우나?** → 백엔드가 대신 호출하고, 워치는 결과만 QR로 그림

---

## 1. 앱 트래픽을 들여다보기 — MITM 프록시와 TLS

### 개념
앱과 서버는 **HTTPS(TLS)**로 암호화 통신한다. 중간에서 보려면
**중간자(MITM, Man-in-the-Middle) 프록시**를 둔다. 프록시가 자신의 인증서로
"가짜 서버" 행세를 하며 앱↔프록시, 프록시↔서버 두 개의 TLS 연결을 만든다.

- 앱이 프록시를 신뢰하려면, 프록시의 **CA 인증서**를 기기가 "신뢰"해야 한다.
- 신뢰 안 하면 `TLS handshake failed: certificate unknown` → 통신 자체가 깨진다.

### 이 프로젝트
- 도구: **mitmproxy** (`uv tool install mitmproxy`, `mitmweb`)
- addon `sniff/taap_sniff.py`가 응답을 훑어 QR/토큰 관련 API만 골라 로그·덤프
- 프록시는 `:8080`, 웹 UI는 `:8081`

### 왜 mitmproxy(Python)인가
캡처는 **일회성 분석 도구**다. MITM 프록시를 Rust로 새로 짜는 건 과잉 —
표준 도구를 쓴다. (최종 산출물인 백엔드만 Rust)

📖 더 볼 것: TLS 핸드셰이크, X.509 인증서 체인, 신뢰 저장소(trust store)

---

## 2. 안드로이드 CA 신뢰 구조 — 왜 실기기는 안 되고 에뮬은 되나

### 개념
안드로이드는 CA를 두 종류로 나눈다:
- **시스템 CA** (`/system/etc/security/cacerts`): OS가 기본 신뢰. 앱도 신뢰.
- **사용자 CA** (사용자가 설치): 브라우저는 신뢰하지만, **Android 7+ 부터 앱은 무시**한다.

즉 실기기에 mitmproxy 인증서를 "사용자 CA"로 깔아도, **앱 트래픽은 복호화 안 된다.**
(이게 삼성 실기기에서 계속 막힌 이유)

### 해결: 루팅 가능한 에뮬레이터 + 시스템 CA 주입
- 에뮬을 `-writable-system`으로 부팅 → `adb root` → `/system`에 쓰기 가능
- mitmproxy CA를 **시스템 CA 위치**에 주입 (`emu/inject_ca.sh`)
- 파일명은 인증서의 `subject_hash_old` 값 + `.0` (안드로이드 규칙)

### 함정: Android 14(API 34)는 안 된다
API 34부터 CA 저장소가 **conscrypt APEX 모듈**로 옮겨져서
`/system/etc/security/cacerts` 주입이 무시된다. 그래서 **API 33(Android 13)** 이미지를
쓴다 — 여기까진 remount + 시스템 store 주입이 그대로 먹힌다.

📖 더 볼 것: Android APEX, SELinux 컨텍스트, verified boot, SSL Pinning(taap엔 없었음)

---

## 3. OAuth2 / OIDC — 로그인과 토큰의 원리

taap 인증은 표준 **OAuth2 Authorization Code + PKCE** + **OIDC**다. 이걸 이해하면
로그인·토큰 갱신·재인계가 전부 설명된다.

### 토큰 3종
| 토큰 | 용도 | 수명 |
|------|------|------|
| **access_token** | API 접근 (scope/client_id 담김) | 5분 |
| **id_token** | 사용자 신원 (roles·email·phone 담김, OIDC) | 5분 |
| **refresh_token** | 위 둘을 재발급 | 길다(rotation) |

### 이 프로젝트의 핵심 발견
- **court API는 `access_token`이 아니라 `id_token`을 Bearer로 요구**한다.
  id_token에만 `roles:[ROLE_MEMBER]`가 있고 서버가 그걸로 인가하기 때문.
  (access_token을 보내면 인증은 되나 권한 부족으로 **403**) ← 큰 삽질 포인트
- **refresh_token rotation**: 갱신할 때마다 새 refresh_token이 발급되고 옛 것은
  (grace period 후) 무효화된다. → 최신 값을 계속 저장해야 한다.

### PKCE (로그인 시)
공개 클라이언트(모바일 앱)는 client_secret을 숨길 수 없다. 대신:
1. 클라가 랜덤 `code_verifier` 생성 → `code_challenge = SHA256(verifier)`
2. authorize 요청에 challenge 전송 → 로그인 후 `code` 받음
3. token 교환 시 `verifier` 제출 → 서버가 challenge와 대조

### 로그인이 왜 자동화하기 어려웠나
`login/email/codes`(OTP 발송)에 **reCAPTCHA Enterprise 토큰**이 필수였다.
reCAPTCHA는 실제 브라우저/웹뷰에서만 생성돼서, 순수 HTTP로 로그인 재현이 막혔다.
→ **인계 방식** 채택: 앱에서 1회 로그인 → refresh_token을 백엔드가 넘겨받아 소유.

📖 더 볼 것: OAuth2 RFC 6749, PKCE RFC 7636, OIDC, JWT 구조(header.payload.signature)

---

## 4. taap API 리버싱 결과

베이스: `https://taapspace.kr` · 인증: `Authorization: Bearer <id_token>`

| 용도 | 요청 | 응답 |
|------|------|------|
| QR 조회 | `GET /api/court/ac/access/qr` | `{data:{qr:{userId, cardSerialNumber}}}` |
| QR 재발급 | `POST /api/court/ac/access/qr` | 위와 동일 (새 값) |
| 토큰 갱신 | `POST /api/court-auth/oauth/token` | `{access_token, id_token, refresh_token}` |
| 로그인 | `authorize → login → email/codes → verification → token` | (reCAPTCHA 필요) |

**QR은 이미지가 아니라 문자열**(`PNPT:...`). 앱이 클라이언트에서 QR로 그린다.
→ 재현 측도 문자열만 받아 워치에서 그리면 된다. (JWT를 base64 디코드하면
`roles`, `device_unique_id`, `phone_number` 등 claim을 직접 볼 수 있다)

---

## 5. 백엔드 (Rust / axum)

`backend/src/main.rs` — 워치가 호출할 HTTP 서버.

### 흐름 (`GET /qr`)
```
refresh_token 읽기 → POST oauth/token(refresh) → id_token + 새 refresh_token
  → 새 refresh_token 저장(rotation) → GET court QR (Bearer id_token) → cardSerialNumber
```

### 배운 점
- **reqwest**: `rustls-tls`로 OpenSSL 의존 제거, `.form()`/`.bearer_auth()`/`.json()`
- **rotation 저장 필수**: 안 하면 다음 요청에서 옛 refresh_token → `invalid_grant`
- **동시성**: refresh 파일 읽기/쓰기를 `Mutex`로 직렬화 (워치 1대라 충분 — 과하게
  안 만든다. 주석 `// ponytail: ...`로 이 한계와 업그레이드 조건을 남겨둠)
- **엔드포인트**: `/qr`, `/health`, `/admin/refresh`(재인계)

📖 더 볼 것: async Rust(tokio), axum extractor/State, Result 에러 매핑

---

## 6. Wear OS 앱

`watch/` — Kotlin + Jetpack Compose for Wear OS.

### 구조 (`MainActivity.kt`)
- 상태 머신: `Idle → Loading → Success(QR) / Failure`
- 버튼 탭 → 코루틴(IO)에서 백엔드 `/qr` 호출 → `cardSerialNumber` → **ZXing**으로
  QR 비트맵 생성 → `Image`로 표시
- HTTP는 `HttpURLConnection`(표준), JSON은 `org.json`(안드 내장) — Retrofit/moshi
  같은 의존성 없이 최소 구성

### 배운 점
- QR은 **흰 배경** 위에 있어야 스캔된다 (워치 기본 배경은 검정)
- `BACKEND_URL`은 `build.gradle.kts`의 `buildConfigField` 한 줄 — 로컬/prod 전환 쉽게
- AGP 8.5는 **JDK 17** 필요 (JDK 26은 못 읽음 → 빌드시 JAVA_HOME 지정)

📖 더 볼 것: Compose 상태관리(remember/mutableStateOf), 코루틴 Dispatchers, ZXing

---

## 7. 배포 (Docker / Render)

`backend/Dockerfile`, `render.yaml`

### 구성
- **멀티스테이지 Docker**: `rust:slim`에서 빌드 → `debian:slim`에 바이너리만 복사
  (rustls라 `ca-certificates`만 필요, OpenSSL 불필요)
- **Render Web Service (Docker)** + **Persistent Disk `/data`**

### 왜 persistent disk가 필수인가
refresh_token은 rotation돼서 **매 요청마다 새 값을 파일에 저장**한다. Render 무료
플랜은 재시작 시 파일시스템이 초기화되므로, disk가 없으면 토큰 체인이 깨진다.
→ 유료 플랜 + disk(`/data/refresh_token.txt`).

### 부트스트랩
최초 refresh_token은 env `TAAP_REFRESH_TOKEN`으로 주입 → 서버가 disk 파일 없으면
env로 초기화. 이후 rotation 값은 disk에 저장(재시작에도 유지).

📖 더 볼 것: Docker layer 캐싱, 12-factor(config를 env로), ephemeral vs persistent fs

---

## 8. 토큰 관리와 재인계

### 문제
백엔드와 taap **앱이 같은 계정**을 쓴다. refresh는 rotation이라, 앱을 열어 쓰면
앱이 refresh를 돌려 **백엔드가 가진 refresh_token을 무효화**한다 → 워치 QR 실패.

### 완전한 해결(보류)
워치 전용 `device_unique_id`로 **별도 로그인** → 독립 세션. 하지만 로그인이
reCAPTCHA+웹뷰 흐름이라 자동화가 까다로워 보류.

### 현실적 해결: 인계 + 재인계
- 평소: 앱을 안 쓰면(워치가 대체) 충돌 없음
- 앱을 쓴 뒤: **재인계** — 새 refresh_token을 백엔드에 밀어넣음
  - 엔드포인트: `POST /admin/refresh` (헤더 `x-admin-key`로 보호)
  - 스크립트: `scripts/reissue.sh` (mitmproxy 덤프에서 추출 → curl 전송)

📖 더 볼 것: 토큰 rotation 전략, refresh token reuse detection, 시크릿 관리

---

## 9. 전체 재현 순서 (직접 해보려면)

1. `uv tool install mitmproxy` + `emu/`로 에뮬(API33)+시스템 CA 주입
2. `mitmweb -s sniff/taap_sniff.py` + 에뮬 프록시 `10.0.2.2:8080`
3. taap 앱 설치(폰에서 APK 추출) → 로그인 → API 덤프 확인
4. `backend/` 로컬 실행으로 `/qr` 검증 → Docker → Render 배포
5. `watch/` 빌드 → 워치에 설치
6. 앱 쓴 뒤 토큰 어긋나면 `scripts/reissue.sh`

## 10. 이 프로젝트에서 배울 수 있는 것 (요약)

- 네트워크: TLS/MITM, 인증서 신뢰 모델
- 모바일 보안: 안드로이드 CA 정책, 에뮬 루팅, APEX
- 인증: OAuth2/OIDC/PKCE, JWT, 토큰 rotation
- 백엔드: Rust async(axum/reqwest/tokio), 상태·동시성
- 프론트: Compose for Wear OS, QR 생성
- 배포: Docker 멀티스테이지, Render, persistent state
- 엔지니어링 태도: 막히면(실기기 CA) 우회로(에뮬) 찾기, 과하게 안 만들기(YAGNI)
