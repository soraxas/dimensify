# Use the official Rust image as a parent image
FROM mcr.microsoft.com/vscode/devcontainers/rust:latest

# Install fish shell
RUN apt-get update && apt-get install -y fish

# Set fish as the default shell
RUN chsh -s /usr/bin/fish
