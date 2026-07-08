# taap-qr 백엔드 — Render(Docker) 배포용. context 는 backend/ 이다.
FROM rust:1-slim AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
# rustls 라 openssl 은 불필요, TLS 루트 인증서만 필요
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /app/target/release/taap-qr /usr/local/bin/taap-qr
# Render 가 PORT 를 주입한다. 서버는 PORT env 를 읽어 0.0.0.0 에 바인딩.
CMD ["taap-qr"]
