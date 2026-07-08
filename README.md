# taap-with-galaxy-watch

갤럭시 워치에서 트리거 → taap이 서버에서 받아오는 QR을 재현해 워치에 띄우기.

**본인 계정 · 본인 기기 트래픽 분석용.** taap 앱이 'QR 생성' 시 자기 서버에 보내는
요청을 캡처해 규격을 알아낸 뒤, 그 요청을 재현하는 백엔드를 만들어 워치가 호출한다.

## 전체 흐름

1. **캡처** — mitmproxy로 taap의 QR 발급 API 규격 파악 (일회성, Python 도구)
2. **재현** — 알아낸 요청을 Rust(reqwest)로 재현 = 워치가 호출할 백엔드
3. **워치 앱** — Wear OS, 트리거 버튼 → 백엔드 호출 → QR 렌더

> 캡처만 Python(mitmproxy)이다. MITM 프록시를 Rust로 새로 짜는 건 과잉. 산출물은 Rust.

## 삽질로 얻은 두 가지 교훈

1. **실기기(비루팅)로는 안 된다.** 안드로이드 7+ 부터 앱은 "사용자가 설치한 CA"를
   신뢰하지 않고 시스템 CA만 믿는다. 삼성 실기기에 인증서를 깔아도 taap 트래픽은
   복호화 안 됨 → **루팅 가능한 에뮬레이터에 CA를 "시스템 CA"로 주입**하는 경로로 간다.
2. **에뮬은 Android 13(API 33)을 써라. API 34(Android 14)는 안 된다.** 14부터 CA
   저장소가 conscrypt APEX로 옮겨져서 `/system/etc/security/cacerts` 주입이 무시된다.
   13까지는 `adb remount` + 시스템 store 주입이 그대로 먹는다. (검증 완료)

## Prerequisites (macOS, Apple Silicon 기준)

```bash
# 1. 캡처 도구 (프로젝트 의존성 아님 — 전역 CLI)
uv tool install mitmproxy

# 2. 에뮬레이터 툴체인
brew install openjdk                          # sudo 불필요 JDK (temurin은 sudo 필요해서 회피)
brew install --cask android-commandlinetools  # sdkmanager, avdmanager

# 3. SDK 컴포넌트 (android-33 google_apis = adb root 되고 CA 주입 쉬운 이미지)
source emu/env.sh
yes | sdkmanager --licenses
sdkmanager "platform-tools" "emulator" "platforms;android-33" \
           "system-images;android-33;google_apis;arm64-v8a"

# 4. AVD 생성
echo no | avdmanager create avd -n taap \
  -k "system-images;android-33;google_apis;arm64-v8a" -d pixel_6
```

SDK 루트: `/opt/homebrew/share/android-commandlinetools` (경로/env 는 `emu/env.sh` 에 고정).

## 실행 방법

### 1) 캡처 프록시 켜기

```bash
mitmweb -s sniff/taap_sniff.py   # 프록시 :8080, 웹 UI http://127.0.0.1:8081
```

`sniff/taap_sniff.py` 는 QR 냄새나는 응답(이미지/base64 PNG/qr·barcode 키워드)만
콘솔에 `[QR 후보]` 로 짚어준다. 판별 로직 자체 테스트: `python3 sniff/taap_sniff.py`.

### 2) 에뮬 부팅 + 시스템 CA 주입 + 프록시

```bash
source emu/env.sh
emulator -avd taap -writable-system -no-snapshot -gpu swiftshader_indirect &
adb wait-for-device                                    # 부팅 대기
emu/inject_ca.sh                                       # mitmproxy CA 를 시스템 store 에 주입 → 자동 재부팅
adb shell settings put global http_proxy 10.0.2.2:8080 # 에뮬 → 호스트 프록시 (10.0.2.2 = 호스트)
```

**검증**: 에뮬 크롬으로 https 접속 후 mitmweb 로그에 `GET/POST https://...` 복호화가
뜨면 성공. (구글 계열 `*.googleapis.com` 은 자체 pinning 이라 실패해도 정상 — 우린 taap 만 본다.)

### 3) taap 설치 → 'QR 생성' → 잡힌 요청을 Copy as curl → Rust 재현

```bash
adb install taap.apk        # 폰에서 추출하거나 APK 직접
```

## 파악된 taap API (캡처 결과)

베이스: `https://taapspace.kr` · 인증: `Authorization: Bearer <JWT>` (cookie 는 GA 뿐, 불필요)

| 용도 | 요청 | 응답 |
|------|------|------|
| QR 조회 | `GET /api/court/ac/access/qr` | `{ data: { qr: { userId, cardSerialNumber } } }` |
| QR 재발급 | `POST /api/court/ac/access/qr` | 위와 동일 (새 cardSerialNumber) |
| 토큰 갱신 | `POST /api/court-auth/oauth/token` | OAuth2 (refresh → 새 access) |

- **QR 은 이미지가 아니라 문자열**(`cardSerialNumber`, 예: `PNPT:...M.<ts>`). 앱이 클라이언트에서 QR 로 렌더 → 재현 측도 문자열만 받아 워치에서 그리면 됨.
- **court API 는 `access_token` 이 아니라 `id_token` 을 Bearer 로 요구한다.** id_token 에만 `roles:[ROLE_MEMBER]` 가 있고 court API 가 그걸로 인가한다. access_token(scope/client_id 만) 을 보내면 **403**. ← 삽질 포인트.
- **토큰 수명 5분, refresh 는 rotation**(매 갱신마다 refresh_token 새로 발급). iss `https://taapspace.kr/api/court-auth`.
- **refresh 체인은 앱과 공유 불가.** 앱이 QR 누를 때마다 rotation 시켜서 백엔드가 가진 refresh_token 이 `invalid_grant` 로 죽는다(그 역도 성립). 검증 시엔 `adb shell am force-stop space.pnpt.fez.taap` 으로 앱을 멈추고 백엔드가 체인을 독점해야 한다. 상시 운용하려면 **워치 백엔드 전용 로그인 세션**이 필요.

## 파일

- `sniff/taap_sniff.py` — mitmproxy addon (QR 후보 하이라이트 + 자체 테스트)
- `emu/env.sh` — JDK/SDK/에뮬 경로 + AVD 이름 고정
- `emu/inject_ca.sh` — 에뮬 시스템 store 에 mitmproxy CA 주입 (root/remount/reboot)

## 현재 진행 상황

- [x] mitmproxy addon + 자체 테스트 (`sniff/taap_sniff.py`)
- [x] 에뮬레이터 툴체인 설치 + AVD 생성 (`emu/env.sh`)
- [x] 에뮬 부팅 + 시스템 CA 주입 (`emu/inject_ca.sh`, API 33) — **복호화 검증 완료**
- [x] taap APK 폰에서 추출 + 에뮬 설치 — **루팅/에뮬 감지 없음, pinning 없음**
- [x] QR 발급 API 캡처 → 규격 확보 (위 표)
- [x] 토큰 갱신(refresh) 흐름 파악 (id_token 사용, rotation)
- [x] **Rust 재현 성공** — refresh → id_token → QR → cardSerialNumber 획득
- [x] refresh 체인 독립화 = **인계 방식** 채택 (앱에서 1회 로그인 → refresh_token 인계 → 앱 미사용)
- [x] **axum 서버** (`backend/`) — 워치는 `GET /qr` 만 호출
- [x] **Wear OS 앱** (`watch/`) — 트리거 버튼 → `/qr` → cardSerialNumber → QR 렌더. 에뮬 검증 완료.

> 완전 독립 세션(워치 전용 device_unique_id 로 별도 로그인)은 court-auth 로그인이 웹뷰
> 전용 흐름(reCAPTCHA + 세션 쿠키 연쇄)이라 브라우저 재현이 까다로워 보류. 앱을 워치가
> 대체(앱 미사용)하는 전제라면 인계 방식으로 충분하다.

## backend 실행

refresh_token 확보(인계): 에뮬/앱에서 taap 로그인 후, `oauth/token` 응답의
`refresh_token` 을 파일로 저장(레포 밖). 이후:

```bash
cd backend
TAAP_REFRESH_FILE=/path/to/refresh_token.txt PORT=8787 cargo run
# GET /qr     → {"userId":..., "cardSerialNumber":"PNPT:..."}
# GET /health → ok
```

refresh_token 은 rotation 되므로 매 요청마다 새 값으로 덮어써진다. **앱을 동시에 쓰면
서로의 refresh 를 무효화**하니 워치 전용으로 둘 것.

### 재인계 (앱을 써서 토큰이 어긋났을 때)

taap 앱을 열어 쓰면 앱이 refresh 를 돌려 백엔드가 가진 refresh_token 이 무효화된다
(워치 QR 이 `invalid_grant`). 이때 새 refresh_token 으로 교체:

```bash
# 1. 에뮬/앱에서 taap 로그인 → mitmproxy 로 oauth/token 응답의 refresh_token 캡처
# 2. 재배포 없이 disk 파일 교체 (ADMIN_KEY 는 Render env 에 설정)
curl -X POST https://taap-qr.onrender.com/admin/refresh \
  -H "x-admin-key: $ADMIN_KEY" -d "<새 refresh_token>"
```

## watch 앱 (Wear OS)

Kotlin + Compose for Wear OS. 트리거 버튼 → 백엔드 `/qr` 호출 → `cardSerialNumber` 를
ZXing 으로 QR 렌더. 백엔드 주소는 `app/build.gradle.kts` 의 `BACKEND_URL`
(에뮬→호스트 `10.0.2.2:8787`, 실기기→PC IP 로 변경).

```bash
cd watch
JAVA_HOME=/opt/homebrew/opt/openjdk@17/libexec/openjdk.jdk/Contents/Home \
  ./gradlew :app:assembleDebug          # AGP 8.5 는 JDK 17 필요 (26 은 못 읽음)
adb -s <wear-serial> install app/build/outputs/apk/debug/app-debug.apk
```

Wear OS 에뮬: `avdmanager create avd -n taap_watch -k "system-images;android-34;android-wear;arm64-v8a" -d wearos_small_round`

## 전체 파이프라인 (완성)

워치 'QR 생성' 탭 → 백엔드 `GET /qr` → refresh → id_token → taap court QR API →
`cardSerialNumber` → 워치가 QR 렌더. **에뮬레이터 end-to-end 검증 완료.**
