FROM mcr.microsoft.com/vscode/devcontainers/rust:1-bullseye

RUN apt-get update \
    && export DEBIAN_FRONTEND=noninteractive \
    && apt-get install -y clang libclang-dev libfreeipmi-dev llvm-dev \
    && apt-get autoremove -y \
    && apt-get clean -y \
    && rm -rf /var/lib/apt/lists/*
