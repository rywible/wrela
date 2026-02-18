FROM ubuntu:24.04

ARG DEBIAN_FRONTEND=noninteractive
ARG RUST_TOOLCHAIN=stable

ENV CARGO_HOME=/opt/cargo
ENV RUSTUP_HOME=/opt/rustup
ENV PATH=/opt/cargo/bin:${PATH}

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    jq \
    make \
    gcc \
    g++ \
    pkg-config \
    libssl-dev \
    clang \
    llvm \
    xz-utils \
    tar \
    unzip \
    python3 \
  && rm -rf /var/lib/apt/lists/*

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal --default-toolchain ${RUST_TOOLCHAIN}

WORKDIR /opt/wrela-bootstrap
COPY . /opt/wrela-bootstrap
RUN cargo build -p wrela --release

WORKDIR /root
CMD ["sleep", "infinity"]
