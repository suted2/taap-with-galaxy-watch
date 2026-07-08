# taap-with-galaxy-watch

갤럭시 워치에서 트리거 → taap이 서버에서 받아오는 QR을 재현해 워치에 띄우기.

**본인 계정 · 본인 기기 트래픽 분석용.** taap 앱이 'QR 생성' 시 자기 서버에 보내는
요청을 캡처해 규격을 알아낸 뒤, 그 요청을 재현하는 백엔드를 만들어 워치가 호출한다.

## 전체 흐름

1. **캡처** — mitmproxy로 taap의 QR 발급 API 규격 파악 (일회성, Python 도구)
2. **재현** — 알아낸 요청을 Rust(reqwest)로 재현 = 워치가 호출할 백엔드
3. **워치 앱** — Wear OS, 트리거 버튼 → 백엔드 호출 → QR 렌더

> 캡처만 Python(mitmproxy)이다. MITM 프록시를 Rust로 새로 짜는 건 과잉. 산출물은 Rust.

## 왜 실기기(비루팅)로는 안 되는가 — 삽질 요약

- 안드로이드 7+ 부터 **앱은 "사용자가 설치한 CA"를 신뢰하지 않는다.** 시스템 CA만 믿음.
- 그래서 삼성 실기기에 mitmproxy 인증서를 깔아도 taap 트래픽은 복호화 안 됨
  (브라우저만 잡히거나, 프록시 때문에 "네트워크 원활하지 않음"만 뜸).
- **결론: 루팅 가능한 에뮬레이터에 인증서를 "시스템 CA"로 주입하는 경로로 간다.**

## Prerequisites (macOS, Apple Silicon 기준)

실제로 설치한 것들:

```bash
# 1. 캡처 도구 (프로젝트 의존성 아님 — 전역 CLI)
uv tool install mitmproxy

# 2. 에뮬레이터 툴체인
brew install openjdk                          # sudo 불필요 JDK (temurin은 sudo 필요해서 회피)
brew install --cask android-commandlinetools  # sdkmanager, avdmanager

# 3. SDK 컴포넌트 (google_apis = adb root 되는 이미지. playstore 이미지는 root 불가)
source emu/env.sh
yes | sdkmanager --licenses
sdkmanager "platform-tools" "emulator" "platforms;android-34" \
           "system-images;android-34;google_apis;arm64-v8a"

# 4. AVD 생성
echo no | avdmanager create avd -n taap \
  -k "system-images;android-34;google_apis;arm64-v8a" -d pixel_6
```

SDK 루트: `/opt/homebrew/share/android-commandlinetools` (경로/env 는 `emu/env.sh` 에 고정).

## 실행 방법

### 1) 캡처 프록시 켜기

```bash
mitmweb -s sniff/taap_sniff.py   # 프록시 :8080, 웹 UI http://127.0.0.1:8081
```

`sniff/taap_sniff.py` 는 QR 냄새나는 응답(이미지/base64 PNG/qr·barcode 키워드)만
콘솔에 `[QR 후보]` 로 짚어준다. 판별 로직 자체 테스트: `python3 sniff/taap_sniff.py`.

### 2) 에뮬레이터 부팅 + 시스템 CA 주입  (진행 예정)

```bash
source emu/env.sh
emulator -avd taap -writable-system -no-snapshot   # 시스템 파티션 쓰기 가능하게
# adb root → CA 해시명으로 /system/etc/security/cacerts 에 push (스크립트화 예정)
```

### 3) taap 설치 → 'QR 생성' → 잡힌 요청을 Copy as curl → Rust 재현

## 현재 진행 상황

- [x] mitmproxy addon + 자체 테스트 (`sniff/taap_sniff.py`)
- [x] 에뮬레이터 툴체인 설치 + AVD 생성 (`emu/env.sh`)
- [ ] 에뮬 부팅 + 시스템 CA 주입 스크립트
- [ ] taap APK 설치 (에뮬이 루팅/에뮬 감지로 막을 수 있음)
- [ ] QR 발급 API 캡처 → 규격 확보
- [ ] Rust 재현 (reqwest) → 워치용 HTTP 서버 (axum)
- [ ] Wear OS 앱
