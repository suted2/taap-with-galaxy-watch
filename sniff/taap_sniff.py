"""taap QR 요청 탐지용 mitmproxy addon.

수백 개 요청 중 'QR 코드를 반환할 법한' 응답만 콘솔에 짚어준다.
캡처 후 나온 curl 을 Rust(reqwest)로 재현하면 그게 워치가 호출할 백엔드가 된다.

실행:
    mitmweb -s sniff/taap_sniff.py      # 브라우저 UI + 필터 로그
    # 폰 프록시를 이 PC:8080 으로, mitm.it 인증서 설치 후 taap 에서 'QR 생성' 클릭
"""
# QR/바코드 냄새나는 힌트들 (소문자 비교)
_HINTS = ("qr", "barcode", "otp", "ticket", "pass", "coupon", "datamatrix", "aztec")
_PNG_B64 = "ivbor"  # base64 PNG 시그니처 (data:image/png;base64,iVBOR...)


def looks_like_qr(url: str, content_type: str, body: str):
    """QR 후보면 이유 문자열, 아니면 None. 순수 함수 — 아래 __main__ 에서 자체 테스트."""
    url, content_type, body = url.lower(), content_type.lower(), body.lower()

    if content_type.startswith("image/"):
        return f"이미지 응답 ({content_type})"
    if _PNG_B64 in body:
        return "본문에 base64 PNG 포함"
    for h in _HINTS:
        if h in url:
            return f"URL에 '{h}'"
        if h in body:
            return f"본문에 '{h}'"
    return None


def response(flow) -> None:  # flow: mitmproxy.http.HTTPFlow (런타임에 주입)
    r = flow.response
    if r is None:
        return
    ctype = r.headers.get("content-type", "")
    # 본문은 앞부분만 (base64 이미지가 수십 KB일 수 있음)
    body = r.get_text(strict=False)[:2000] if not ctype.startswith("image/") else ""
    reason = looks_like_qr(flow.request.pretty_url, ctype, body)
    if reason is None:
        return

    req = flow.request
    print("\n" + "=" * 60)
    print(f"[QR 후보] {reason}")
    print(f"  {req.method} {req.pretty_url}")
    print(f"  status {r.status_code}  content-type {ctype}")
    # Rust 재현용: mitmproxy UI 에서 우클릭 'Copy as curl' 이 더 정확하지만, 빠른 참고용
    auth = req.headers.get("authorization", "")
    if auth:
        print(f"  authorization: {auth[:40]}...")
    print("=" * 60)


if __name__ == "__main__":
    # ponytail: 프레임워크 없이 assert 자체 테스트 하나
    assert looks_like_qr("https://x/api/qr/create", "application/json", "{}") == "URL에 'qr'"
    assert looks_like_qr("https://x/api/gen", "image/png", "") == "이미지 응답 (image/png)"
    assert looks_like_qr("https://x/api/gen", "application/json",
                         '{"img":"data:image/png;base64,iVBORw0"}') == "본문에 base64 PNG 포함"
    assert looks_like_qr("https://x/api/user", "application/json", '{"name":"kim"}') is None
    assert looks_like_qr("https://x/v1/barcode", "text/plain", "") == "URL에 'barcode'"
    print("ok")
