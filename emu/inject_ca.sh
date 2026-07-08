#!/usr/bin/env bash
# mitmproxy CA 를 에뮬레이터 시스템 신뢰 store 에 주입한다.
# 전제: -writable-system 으로 부팅된 에뮬레이터 (adb root/remount 가능).
# 사용: source emu/env.sh && emu/inject_ca.sh
set -euo pipefail
CA="${HOME}/.mitmproxy/mitmproxy-ca-cert.pem"
[ -f "$CA" ] || { echo "CA 없음: $CA (mitmweb 한 번 실행하면 생성됨)"; exit 1; }

# 안드로이드 CA store 파일명은 = subject_hash_old 값 + .0
HASH=$(openssl x509 -inform PEM -subject_hash_old -noout -in "$CA")
echo "CA hash: $HASH"

adb root >/dev/null; adb wait-for-device
adb remount >/dev/null 2>&1 || true
adb push "$CA" "/system/etc/security/cacerts/${HASH}.0"
adb shell chmod 644 "/system/etc/security/cacerts/${HASH}.0"

# Android 14: conscrypt APEX store 에도 넣어야 앱이 신뢰하는 경우가 있음
if adb shell '[ -d /apex/com.android.conscrypt/cacerts ]' 2>/dev/null; then
  adb shell "cp /system/etc/security/cacerts/${HASH}.0 /apex/com.android.conscrypt/cacerts/${HASH}.0 2>/dev/null" || true
fi

echo "주입 완료. 재부팅한다..."
adb reboot
