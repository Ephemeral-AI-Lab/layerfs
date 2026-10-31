FROM node:24.21.0-bookworm

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        git \
        lsof \
        procps \
        python3 \
        python3-pip \
        python3-venv \
    && rm -rf /var/lib/apt/lists/*
