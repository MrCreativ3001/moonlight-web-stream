FROM ubuntu:24.04

ENV MOONLIGHT_WEB_PATH=/moonlight-web

# Create a non-root user
RUN groupadd -r moonlight && useradd -r -g moonlight -d ${MOONLIGHT_WEB_PATH} -s /sbin/nologin moonlight

# Create / Go to app directory
WORKDIR ${MOONLIGHT_WEB_PATH}

# Download the release binary
ADD https://github.com/MrCreativ3001/moonlight-web-stream/releases/latest/download/moonlight-web-x86_64-unknown-linux-gnu.tar.gz /tmp/app.tar.gz

# Extract it, create server directory, and change ownership
RUN tar -xvzf /tmp/app.tar.gz --strip-components=1 -C ${MOONLIGHT_WEB_PATH}/ && \
    rm /tmp/app.tar.gz && \
    mkdir -p ${MOONLIGHT_WEB_PATH}/server && \
    chown -R moonlight:moonlight ${MOONLIGHT_WEB_PATH} && \
    chmod +x ${MOONLIGHT_WEB_PATH}/web-server

# Create a volume with the config.json and data.json
VOLUME ${MOONLIGHT_WEB_PATH}/server

# Expose the port (adjust if you changed it)
ENV WEBRTC_PORT_RANGE=40000:40100
EXPOSE 8080 40000-40100/udp

# Create an entrypoint
COPY entrypoint.sh /entrypoint.sh
RUN chown moonlight:moonlight /entrypoint.sh && \
    chmod +x /entrypoint.sh

# Switch to the non-root user
USER moonlight

# Run the binary
ENTRYPOINT ["/entrypoint.sh"]
