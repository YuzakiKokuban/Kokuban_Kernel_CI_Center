FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    git \
    libncurses5-dev \
    bc \
    bison \
    flex \
    libssl-dev \
    p7zip-full \
    pigz \
    lz4 \
    cpio \
    curl \
    wget \
    libelf-dev \
    dwarves \
    jq \
    lld \
    pahole \
    libdw-dev \
    unzip \
    zip \
    ca-certificates \
    shellcheck \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

# The toolchain is installed up front so the container never has to fetch it at run time;
# keep the version in sync with rust-toolchain.toml.
# --component takes a single comma-separated argument, not a space-separated list.
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal \
    --default-toolchain 1.92.0 \
    --component rustfmt,clippy

ENV PATH="/root/.cargo/bin:${PATH}"
WORKDIR /workspace

CMD ["/bin/bash"]
