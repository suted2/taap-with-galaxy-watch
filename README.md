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
- **access token = JWT, 수명 5분.** iss `https://taapspace.kr/api/court-auth`. 계속 쓰려면 refresh 필요 → 백엔드가 토큰 갱신 담당.

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
- [ ] 토큰 갱신(refresh) 요청 body 캡처 → 자동 갱신 구현
- [ ] Rust 재현 (reqwest) → 워치용 HTTP 서버 (axum)
- [ ] Wear OS 앱
