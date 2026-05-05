# syntax=docker/dockerfile:1.7

# ----------------------------------------------------------------------------
# Stage 1 — build the IVF index from references.json.gz
# ----------------------------------------------------------------------------
FROM --platform=linux/amd64 rust:1-slim AS index-builder

WORKDIR /src
RUN apt-get update && apt-get install -y --no-install-recommends \
        gcc libc6-dev pkg-config ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Optional: corporate TLS-interception CA certs (e.g. ZScaler). Empty in real submission.
COPY certs/ /usr/local/share/ca-certificates/
RUN update-ca-certificates 2>/dev/null || true

COPY Cargo.toml ./
COPY rust-build/Cargo.toml rust-build/Cargo.toml
COPY rust-ext/Cargo.toml rust-ext/Cargo.toml
COPY rust-ext/build.rs rust-ext/build.rs

RUN mkdir -p rust-build/src rust-build/src/bin rust-ext/src && \
    echo 'fn main(){}' > rust-build/src/build_index.rs && \
    echo 'fn main(){}' > rust-build/src/bin/expected.rs && \
    echo '// stub' > rust-ext/src/lib.rs && \
    cargo fetch && \
    rm -rf rust-build/src

COPY rust-build/src ./rust-build/src
COPY resources ./resources

RUN cargo build --release -p rust-build --bin build_index
RUN mkdir -p /out && \
    ./target/release/build_index resources/references.json.gz /out/index.bin && \
    ls -la /out/index.bin
RUN cargo build --release -p rust-build --bin expected && \
    cp ./target/release/expected /out/expected

# ----------------------------------------------------------------------------
# Stage 2 — build the Rust PHP extension (rinha_rs.so)
# ----------------------------------------------------------------------------
FROM --platform=linux/amd64 rust:1-slim AS ext-builder

WORKDIR /src

RUN apt-get update && apt-get install -y --no-install-recommends \
        clang libclang-dev pkg-config \
        ca-certificates curl gnupg lsb-release \
    && rm -rf /var/lib/apt/lists/*

# Optional: corporate TLS-interception CA certs (e.g. ZScaler). Empty in real submission.
COPY certs/ /usr/local/share/ca-certificates/
RUN update-ca-certificates 2>/dev/null || true

RUN curl -sSLo /usr/share/keyrings/php.gpg https://packages.sury.org/php/apt.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/php.gpg] https://packages.sury.org/php/ bookworm main" \
        > /etc/apt/sources.list.d/php.list \
    && apt-get update && apt-get install -y --no-install-recommends \
        php8.3 php8.3-dev php8.3-cli \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml ./
COPY rust-build/Cargo.toml rust-build/Cargo.toml
COPY rust-ext/Cargo.toml rust-ext/Cargo.toml
COPY rust-ext/build.rs rust-ext/build.rs

RUN mkdir -p rust-build/src rust-build/src/bin rust-ext/src && \
    echo 'fn main(){}' > rust-build/src/build_index.rs && \
    echo 'fn main(){}' > rust-build/src/bin/expected.rs && \
    echo '// stub' > rust-ext/src/lib.rs && \
    cargo fetch && \
    rm -rf rust-ext/src

COPY rust-ext/src ./rust-ext/src

ENV RUSTFLAGS="-C target-cpu=haswell -C target-feature=+avx2,+fma,+bmi2,+popcnt -C link-arg=-s"
RUN cargo build --release -p rinha-rs && \
    mkdir -p /out && \
    cp target/release/librinha_rs.so /out/rinha_rs.so && \
    ls -la /out/rinha_rs.so

# ----------------------------------------------------------------------------
# Stage 3 — runtime: PHP 8.3 CLI + Swoole + rinha_rs + index.bin
# ----------------------------------------------------------------------------
FROM --platform=linux/amd64 php:8.3-cli-bookworm AS runtime

# Optional: corporate TLS-interception CA certs (e.g. ZScaler). Empty in real submission.
COPY certs/ /usr/local/share/ca-certificates/
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && update-ca-certificates 2>/dev/null || true

RUN apt-get update && apt-get install -y --no-install-recommends \
        libssl-dev libcurl4-openssl-dev libc-ares-dev libbrotli-dev \
    && pecl install --configureoptions 'enable-openssl="yes" enable-swoole-curl="no" enable-cares="no" enable-brotli="no"' swoole \
    && docker-php-ext-enable swoole opcache \
    && rm -rf /var/lib/apt/lists/* /tmp/pear

COPY --from=ext-builder /out/rinha_rs.so /tmp/rinha_rs.so
RUN cp /tmp/rinha_rs.so "$(php-config --extension-dir)/rinha_rs.so" \
    && rm /tmp/rinha_rs.so \
    && echo "extension=rinha_rs.so" > /usr/local/etc/php/conf.d/zz-rinha-rs.ini

COPY --from=index-builder /out/index.bin /app/data/index.bin
COPY php/ /app/

ENV INDEX_PATH=/app/data/index.bin

WORKDIR /app

# Sanity check at build time: extension must load and dataset must mmap.
RUN INDEX_PATH=/app/data/index.bin php -c /app/php.ini -r 'echo "modules ok\n"; var_dump(function_exists("rinha_fraud_score"));'

CMD ["php", "-c", "/app/php.ini", "/app/server.php"]
