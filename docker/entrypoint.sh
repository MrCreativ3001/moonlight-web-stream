#!/bin/bash
set -e

# Set default values
TIMEZONE=${TIMEZONE:-"America/New_York"}
SERVER_IP=${SERVER_IP:-"192.168.1.100"}
WEB_PORT=${WEB_PORT:-"8080"}
PAIR_DEVICE_NAME=${PAIR_DEVICE_NAME:-"roth"}

# Set timezone
if [ -n "$TIMEZONE" ]; then
  ln -snf /usr/share/zoneinfo/"$TIMEZONE" /etc/localtime
  echo "$TIMEZONE" > /etc/timezone
fi

# Make sure the server folder exists
mkdir -p ${MOONLIGHT_WEB_PATH}/server

# Copy default config if none exists
if [ ! -f /moonlight-web/server/config.json ]; then
    cp ${MOONLIGHT_WEB_PATH}/defaults/config.json /moonlight-web/server/config.json
fi

# Update config.json with environment variables
if [ -f "/moonlight-web/server/config.json" ]; then
  # Use jq to update the config file with environment variables
  jq --arg ip "$SERVER_IP" --arg port "$WEB_PORT" '
    .webrtc.nat_1_1_mapping_ip = $ip |
    .bind_address = "0.0.0.0:" + $port
  ' /moonlight-web/server/config.json > /moonlight-web/server/config.json.tmp && mv /moonlight-web/server/config.json.tmp /moonlight-web/server/config.json
fi

# Run main application
exec ${MOONLIGHT_WEB_PATH}/web-server