
# Docker for Moonlight-Web

Run with
```sh
docker run -d -p 8080:8080 -p 40000-40100:40000-40100/udp -e WEBRTC_PORT_RANGE=40000:40100 -e WEBRTC_NAT_1TO1_HOST=YOUR_LAN_IP ghcr.io/spacedouut/moonlight-web-stream-simplified:latest
```
and replace `YOUR_LAN_IP` with the device ip address of the local network.

`WEBRTC_PORT_RANGE` must match the udp port mapping.

Images are published to GHCR on every push to `master` (`:latest`, `:master`) and on `v*` tags.

The container has no authentication; put a reverse proxy with authentication in front of it (see [Authentication](../README.md#authentication)).

# Running with a TURN server

1. Copy the [docker-compose.with-turn.yaml](./docker-compose.with-turn.yaml) into your own `docker-compose.yaml`.

2. Create a new `.env` file with:
```dotenv
ML_WEB_VERSION=latest

LAN_ADDRESS=127.0.0.1 # Change this to the device ip address of the local network.

TURN_URL=myturn.com
TURN_USERNAME=myrandomuser
TURN_CREDENTIAL=myrandompass
```

3. Run with docker-compose
```sh
docker compose up
```
