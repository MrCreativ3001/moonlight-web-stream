
# Docker for Moonlight-Web

Run with
```sh
docker run -d -p 8080:8080 -p 40000-40100:40000-40100/udp -e WEBRTC_NAT_1TO1_HOST=YOUR_LAN_IP -e WEBRTC_PORT_RANGE=40000:40100 mrcreativ3001/moonlight-web-stream:latest
```
and replace `YOUR_LAN_IP` with the device ip address of the local network.

`WEBRTC_PORT_RANGE` must match the udp port mapping. Environment variables
override `server/config.json`, so if you prefer configuring
`webrtc.port_range` in the config file (e.g. different ranges for multiple
instances), don't set the environment variable — it is no longer baked into
the image.

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