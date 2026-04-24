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

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --profile minimal

ENV PATH="/root/.cargo/bin:${PATH}"
WORKDIR /workspace

CMD ["/bin/bash"]
