#!/usr/bin/env bash
# taap refresh_token 재인계.
#
# taap 앱을 써서 rotation 이 어긋나면(워치 QR 이 invalid_grant) 실행한다.
# mitmproxy 덤프에서 가장 최근 refresh_token 을 뽑아 Render 백엔드로 밀어넣는다.
#
# 전제:
#   1) mitmweb -s sniff/taap_sniff.py 로 캡처 중 (에뮬 프록시 10.0.2.2:8080)
#   2) taap 앱에서 로그인/사용해 oauth/token(refresh) 이 덤프에 잡힌 상태
#   3) ADMIN_KEY 환경변수 설정 (Render 에 넣은 값과 동일)
#
# 사용:
#   ADMIN_KEY=xxxx scripts/reissue.sh
set -euo pipefail

DUMP="${TAAP_DUMP:-/tmp/qr_capture.txt}"
URL="${TAAP_BACKEND:-https://taap-qr.onrender.com}"
KEY="${ADMIN_KEY:?ADMIN_KEY 환경변수가 필요합니다 (Render 에 설정한 값)}"

[ -f "$DUMP" ] || { echo "덤프 파일 없음: $DUMP (mitmweb 캡처 먼저)"; exit 1; }

# 덤프의 oauth/token 200 응답들 중 마지막 refresh_token 추출
RT=$(python3 - "$DUMP" <<'PY'
import sys, re, json
dump = open(sys.argv[1]).read()
resps = re.findall(r'--- response 200[^\n]*\n(\{.*?\})\n', dump)
tok = None
for r in resps:
    try:
        j = json.loads(r)
        if 'refresh_token' in j:
            tok = j['refresh_token']   # 가장 최근 것으로 갱신
    except Exception:
        pass
print(tok or '', end='')
PY
)

[ -n "$RT" ] || { echo "덤프에서 refresh_token 을 못 찾음 (앱에서 로그인/사용 후 다시)"; exit 1; }
echo "refresh_token 추출됨 (길이 ${#RT})"

echo -n "Render 로 전송... "
curl -fsS -X POST "$URL/admin/refresh" -H "x-admin-key: $KEY" -d "$RT"
echo

echo -n "동작 확인: "
curl -s -o /dev/null -w "/qr HTTP %{http_code}\n" "$URL/qr"
