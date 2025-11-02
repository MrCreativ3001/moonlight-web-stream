FROM debian:bookworm-slim

ENV MOONLIGHT_WEB_PATH=/moonlight-web

# Install required utilities
RUN apt-get update && apt-get install -y --no-install-recommends \
    jq \
    tzdata \
    && rm -rf /var/lib/apt/lists/*

# Create / Go to app directory
WORKDIR ${MOONLIGHT_WEB_PATH}

# Download the release binary
ADD https://github.com/MrCreativ3001/moonlight-web-stream/releases/download/v1.6/moonlight-web-x86_64-unknown-linux-gnu.tar.gz /tmp/app.tar.gz

# Extract it
RUN tar -xvzf /tmp/app.tar.gz --strip-components=1 -C ${MOONLIGHT_WEB_PATH}/ &&\
    rm /tmp/app.tar.gz

# Create a location with config defaults
RUN mkdir ${MOONLIGHT_WEB_PATH}/defaults
COPY ./default-config.json ${MOONLIGHT_WEB_PATH}/defaults/config.json

# Create a volume with the default config.json and data.json
VOLUME ${MOONLIGHT_WEB_PATH}/server

# Expose the port (adjust if you changed the config)
EXPOSE 8080 40000-40100/udp

# Create an entrypoint
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Run the binary
ENTRYPOINT ["/entrypoint.sh"]