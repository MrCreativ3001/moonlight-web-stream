
# Moonlight Web
An unofficial [Moonlight Client](https://moonlight-stream.org/) allowing you to stream your pc to the Web.
It hosts a Web Server which will forward [Sunshine](https://docs.lizardbyte.dev/projects/sunshine/latest/) traffic to a Browser using the [WebRTC Api](https://webrtc.org/).

![An image displaying: PC with sunshine and moonlight web installed, a browser making requests to it](/readme/structure.png)

> **Note**
> Documentation for the current latest release is available on the [`v2.10.0`](https://github.com/MrCreativ3001/moonlight-web-stream/tree/v2.10.0) branch. The `master` branch contains the latest development version and may not match the published documentation.

## Overview

- [Limitations](#limitations)
- [Installation](#installation)
  - [Manual Installation](#install-manually)
  - [Docker Installation](docker/README.md)
- [Setup](#setup)
  - [Streaming over the Internet](#streaming-over-the-internet)
  - [Configuring https](#configuring-https)
  - [Proxying via Apache 2](#proxying-via-apache-2)
  - [Authentication with a Reverse Proxy](#authentication-using-a-reverse-proxy)
  - [Using Web Socket Transport](#using-websocket-transport)
- [Config](#config)
- [Migrating to v2](#migrating-to-v2)
- [Migrating to v3](#migrating-to-v3)
- [Contributors](#contributors)
- [Building](#building)

## Limitations
- Features that only work in a [Secure Context](https://developer.mozilla.org/en-US/docs/Web/Security/Secure_Contexts#:~:text=They%20must%20be,be%20considered%20deprecated.) -> [How to configure a Secure Context / https](#configuring-https)
  - Controllers: [Gamepad API](https://developer.mozilla.org/en-US/docs/Web/API/Gamepad_API)
  - Keyboard Lock (allows to capture almost all keys also OS Keys): [Experimental Keyboard Lock API](https://developer.mozilla.org/en-US/docs/Web/API/Keyboard_API)
  - Web Socket Transport because of the [Web Codecs Api](https://developer.mozilla.org/en-US/docs/Web/API/VideoDecoder)

## Installation

You can install it [manually](#install-manually) or with [docker](docker/README.md)

### Install Manually

1. Install [Sunshine](https://github.com/LizardByte/Sunshine/blob/v2025.628.4510/docs/getting_started.md)

2. Download the [compressed archive](https://github.com/MrCreativ3001/moonlight-web-stream/releases/latest) for your platform and uncompress it or [build it yourself](#building)

3. Run the "web-server" executable

4. Go to `localhost:8080` and view the web interface. You can also the change [bind address](#bind-address).

> **NOTE**
> A `config.json` file is **not generated automatically on first startup**.
> If you plan to follow the configuration guides below, you will need to create a config.json with `./web-server config generate`

## Setup

1. Add a new user by typing in your name and password. The first user to login will be created and will be the admin.

2. Add a new pc (<img src="web/resources/ic_add_to_queue_white_48px.svg" alt="icon" style="height:1em; vertical-align:middle;">) with the address as `localhost` and leave the port empty (if you've got the default port)

2. Pair your pc by clicking on the host (<img src="web/resources/desktop_windows-48px.svg" alt="icon" style="height:1em; vertical-align:middle;">). Then enter the code in sunshine

3. Launch an app

### Streaming over the Internet

1. Forward the web server port on your router (default is `8080`; HTTP is `80`, HTTPS is `443`).  
   You can configure this using the [`bind_address`](#bind-address) option.

When running on a **local network**, WebRTC peers usually negotiate connections without any issues.

When streaming **over the Internet**, the built-in STUN servers will attempt to establish a direct peer-to-peer WebRTC connection. This works for most home networks, but may fail in more restrictive environments (for example, corporate firewalls or strict NAT configurations).

If a direct WebRTC connection cannot be established, you have a few options:

1. **Use a TURN server (recommended)**  
   Configure a [TURN server](#configure-a-turn-server) to reliably relay WebRTC traffic when direct peer-to-peer connections are blocked.

2. **Forward ports directly**  
   You can [forward the required ports](#port-forward) on your router.  
   Note that this may still fail if the firewall blocks UDP traffic.

3. **Use WebSocket transport as a fallback**  
   WebSockets use standard HTTP/HTTPS ports and are often allowed even in very restrictive networks, making them a good fallback option when WebRTC negotiation fails. See [Using WebSocket Transport](#using-websocket-transport).


#### Configure a turn server
1. Host and configure a turn server like [coturn](https://github.com/coturn/coturn) or use other services to host one for you.

2. Add your turn server to your WebRTC Ice Server list
```json
{
    "webrtc": {
        "ice_servers": [
            {
                "urls": [
                    "stun:stun.l.google.com:19302",
                    "stun:stun1.l.google.com:3478",
                    "stun:stun.l.google.com:5349"
                ]
            },
            {
                "urls": [
                        "turn:yourip.com:3478?transport=udp",
                        "turn:yourip.com:3478?transport=tcp",
                        "turn:yourip.com:5349?transport=tcp"
                ],
                "username": "your username",
                "credential": "your credential"
            }
        ]
    }
}
```
Some (business) firewalls might be very strict and only allow tcp on port 443 for turn connections if that's the case also bind the turn server on port 443 and add `"turn:yourip.com:443?transport=tcp"` to the url's list.

#### Port forward

1. Set the [WebRTC Port Range](#webrtc-port-range) to a fixed range
```json
{
    "webrtc": {
        "port_range": {
            "min": 40000,
            "max": 40010
        }
    }
}
```
2. Forward the port range specified in the previous step as `udp`.
If you're using Windows Defender make sure to allow NAT Traversal. Important: If your firewall blocks udp connections this won't work and you need to host a [turn server](#configure-a-turn-server)

3. Configure [WebRTC Nat 1 To 1](#webrtc-nat-1-to-1-ips) to advertise your [public ip](https://whatismyipaddress.com/) (Optional: WebRTC stun servers can usually automatically detect them):
```json
{
    "webrtc": {
        "nat_1to1": {
            "ice_candidate_type": "host",
            "ips": [
                "74.125.224.72"
            ]
        }
    }
}
```

It might be helpful to look what kind of nat your pc is behind:
- [Nat Checker](https://www.checkmynat.com/)

### Configuring https
You can configure https directly with the Moonlight Web Server.

1. You'll need a private key and a certificate.

You can generate a self signed certificate with this python script [generate_certificate.py](generate_certificate.py):

```sh
pip install pyOpenSSL
python ./generate_certificate.py
```

2. Copy the files `server/key.pem` and `server/cert.pem` into your `server` directory.

3. Configure the [private key](#https-certificates) and [certificate](#https-certificates) https using the certificates
```json
{
    "web_server": {
        "certificate": {
            "private_key_pem": "./server/key.pem",
            "certificate_pem": "./server/cert.pem"
        }
    }
}
```

### Proxying via Apache 2
It's possible to proxy the Moonlight Website using [Apache 2](https://httpd.apache.org/).

Note:
When you want to use https, the Moonlight Website should use http so that Apache 2 will handle all the https encryption.

1. Enable the modules `mod_proxy`, `mod_proxy_wstunnel`

```sh
sudo a2enmod mod_proxy mod_proxy_wstunnel
```

2. Create a new file under `/etc/apache2/conf-available/moonlight-web.conf` with the content:
```
# Example subpath "/moonlight" -> To connect you'd go to "http://yourip.com/moonlight/"
Define MOONLIGHT_SUBPATH /moonlight
# The address and port of your Moonlight Web server
Define MOONLIGHT_STREAMER YOUR_LOCAL_IP:YOUR_PORT

ProxyPreserveHost on
        
# Important: This WebSocket will help negotiate the WebRTC Peers
<Location ${MOONLIGHT_SUBPATH}/api/host/stream/web_socket>
        ProxyPass ws://${MOONLIGHT_STREAMER}${MOONLIGHT_SUBPATH}/api/host/stream/web_socket
        ProxyPassReverse ws://${MOONLIGHT_STREAMER}${MOONLIGHT_SUBPATH}/api/host/stream/web_socket
</Location>

ProxyPass ${MOONLIGHT_SUBPATH}/ http://${MOONLIGHT_STREAMER}${MOONLIGHT_SUBPATH}/
ProxyPassReverse ${MOONLIGHT_SUBPATH}/ http://${MOONLIGHT_STREAMER}${MOONLIGHT_SUBPATH}/
```

3. Enable the created config file
```sh
sudo a2enconf moonlight-web
```

4. Change [config](#config) to include the [prefixed path](#url-path-prefix)
```json
{
    "web_server": {
        "url_path_prefix": "/moonlight"
    }
}
```

5. Use https with a certificate (Optional)

### Authentication using a Reverse Proxy
Authentication with a reverse proxy works by the proxy adding custom headers to the request of the user. In this example the username header is named `X-Forwarded-User`.

<b>Make sure that the header is not changeable by any external request and only the proxy can set this header.</b>

Enable proxy authentication by setting the [forwarded header username](#forwarded-header-username) option.
By default the [auto create missing user](#forwarded-header-auto-create-missing-user) option is turned on even if it's not specified in the config.
```json
{
    "web_server": {
        "forwarded_header": {
            "username_header": "X-Forwarded-User",
            "auto_create_missing_user": true
        }
    }
}
```

### Using WebSocket Transport

In some networks (for example, corporate or highly restricted environments), establishing a WebRTC connection can be difficult or may not work at all.
To support these cases, you can stream data over **WebSockets** using the same HTTP or HTTPS port that your server already exposes.

You can force the Web Sockets by setting the `Data Transport` option to `Web Sockets`.

There are a few important things to be aware of when using WebSockets for streaming:

- The browser’s `VideoDecoder` API is only available when your site is served over **HTTPS** (a secure context).

- If the application is **not** running in a secure context, it will fall back to an older video playback API, which may introduce noticeable latency.

- If this is unacceptable and you know what you're doing, you can use a software-based decoder:
  - Build a custom version of [openh264](https://github.com/MrCreativ3001/openh264-js) for the browser.
  - This custom build allows video frames to be decoded directly in the browser without relying on the `VideoDecoder` API.
  - After building, copy the generated `decoder.js` file to:
    ```
    static/libopenh264/decoder.js
    ```
  - The decoder will be detected and used automatically.

## Config
The config file is under `server/config.json` relative to the executable.
Here are the most important settings for configuring Moonlight Web.

Most options have command line arguments or environment variables associated with them.
```sh
./web-server help
```

Generate the config with:
```sh
./web-server config generate
```

For a full list of values look into the [Rust Config module](src/config.rs).

### Bind Address 
The address and port the website will run on

Environment Variable: `BIND_ADDRESS=0.0.0.0:8080`

```json
{
    "web_server": {
        "bind_address": "0.0.0.0:8080"
    }
}
```

### Default User
The user id which is selected by default when providing no login.
Go into the Admin Panel and look for the user id of the user you want to make the default.

```json
{
    "web_server": {
        "default_user_id": 1284358932
    }
}
```

### Https Certificates
If enabled the web server will use https with the provided certificate data

Environment Variables:
- `SSL_PRIVATE_KEY=./server/key.pem`
- `SSL_CERTIFICATE=./server/cert.pem`

```json
{
    "web_server":{
        "certificate": {
            "private_key_pem": "./server/key.pem",
            "certificate_pem": "./server/cert.pem"
        }
    }
}
```

### WebRTC Port Range
This will set the port range on the web server used to communicate when using WebRTC

Environment Variable: `WEBRTC_PORT_RANGE=40000:40010`

```json
{
    "webrtc": {
        "port_range": {
            "min": 40000,
            "max": 40010
        }
    }
}
```

### WebRTC Ice Servers
A list of ice servers for webrtc to use.

```json
{
    "webrtc": {
        "ice_servers": [
            {
                "urls": [
                    "stun:stun.l.google.com:19302",
                    "stun:stun1.l.google.com:3478",
                    "stun:stun.l.google.com:5349"
                ]
            }
        ]
    }
}
```

You can also set ice servers with environment variables:
```dockerfile
ENV WEBRTC_ICE_SERVER_0_URL=stun:stun.l.google.com:5349
ENV WEBRTC_ICE_SERVER_0_USERNAME=name
ENV WEBRTC_ICE_SERVER_0_CREDENTIAL=cred
ENV WEBRTC_ICE_SERVER_1_URL=stun:stun1.l.google.com:5349
```
will currespond to the ice server
```json
{
    "webrtc": {
        "ice_servers": [
            {
                "urls": [
                    "stun:stun.l.google.com:19302",
                ],
                "username": "name",
                "credential": "cred"
            },
            {
                "urls": [
                    "stun:stun1.l.google.com:5349"
                ]
            }
        ]
    }
}
```

You can disable all default ice servers with the cli argument `--disable-default-webrtc-ice-servers` or the environment variable `DISABLE_DEFAULT_WEBRTC_ICE_SERVERS`.

### WebRTC Ice Server Script
The given script will be executed on every stream start to dynamically generate ice servers.

```json
{
    "webrtc": {
        "ice_server_script": "./server/ice_server_script.sh"
    }
}
```

The script must return a json list that follows the same structure as the [WebRTC Ice Servers](#webrtc-ice-servers) config option.

Linux example using a bash script:
```sh
#!/bin/bash

# Assign arguments to variables
username="user"
credential="cred"

# Output the json into stdout
echo "[
    {
        \"urls\": [
            \"turn:example.com\"
        ],
        \"username\": \"$username\",
        \"credential\": \"$credential\"
    }
]"
```

Windows example using a batch script:
```bat
@echo off

:: Assign arguments to variables
set "username=user"
set "credential=cred"

:: Output the JSON with dynamic values
echo [
echo     {
echo         "urls": [
echo             "turn:example.com"
echo         ],
echo         "username": "%username%",
echo         "credential": "%credential%"
echo     }
echo ]
```

### WebRTC Nat 1 to 1 ips
This will advertise the ip as an ice candidate on the web server.
It's recommended to set this but stun servers should figure out the public ip.

`ice_candidate_type`:
- `host` -> This is the ip address of the server and the client can connect to
- `srflx` -> This is the public ip address of this server, like an ice candidate added from a stun server.

```json
{
    "webrtc": {
        "nat_1to1": {
            "ice_candidate_type": "host",
            "ips": [
                "74.125.224.72"
            ]
        }
    }
}
```

You can also use the cli argument `--webrtc-nat-1to1-host` or environment variable `WEBRTC_NAT_1TO1_HOST` to use a ip as a host candidate type. This will do the same as the json above.
```dockerfile
ENV WEBRTC_NAT_1TO1_HOST=74.125.224.72
```

### WebRTC Network Types
This will set the network types allowed by webrtc.
<br>Allowed values:
- udp4: All udp with ipv4
- udp6: All udp with ipv6
- tcp4: All tcp with ipv4
- tcp6: All tcp with ipv6

```json
{
    "webrtc": {
        "network_types": [
            "udp4",
            "udp6",
        ]
    }
}
```

### Url Path Prefix
This is useful when rerouting the web page using services like [Apache 2](#proxying-via-apache-2).
Will always append the prefix to all requests made by the website.

Environment Variable: `PATH_PREFIX=/moonlight`

```json
{
    "web_server": {
        "url_path_prefix": "/moonlight"
    }
}
```

### Forwarded Header Username
The header that will give the authenticated username to this web server.

```json
{
    "web_server": {
        "forwarded_header": {
            "username_header": "X-Forwarded-User"
        }
    }
}
```

### Forwarded Header Auto Create Missing User
Automatically create a new user when the requested user specified in the [username_header](#forwarded-header-username) is not found.

```json
{
    "web_server": {
        "forwarded_header": {
            "auto_create_missing_user": true
        }
    }
}
```

## Migrating to v2
1. Some config options have changed so backup your old config by renaming it to something like `old_config.json`.

2. Start the web server which will generate the new config.

3. Move your configurations to the new config

4. The first user to login will be created and will be an admin. All previously stored hosts will be moved to this user.

Other changes:
- Proxy path changed:
  - change all instances of `ProxyPass ${MOONLIGHT_SUBPATH}/ http://${MOONLIGHT_STREAMER}/`<br> to `ProxyPass ${MOONLIGHT_SUBPATH}/ http://${MOONLIGHT_STREAMER}${MOONLIGHT_SUBPATH}/`
  - [Proxying via Apache 2](https://github.com/MrCreativ3001/moonlight-web-stream/tree/v2?tab=readme-ov-file#proxying-via-apache-2)

## Migrating to v3
Changes:
- replaced openssl by rustls
  - This makes older v1 and v2 server certificates incompatible
  - When using a self signed certificate, regenerate it using the updated [generate_certificate.py](./generate_certificate.py)
- replaced moonlight-common-c by moonlight-common-rust
  - This could make older Nvidia GameStream and Sunshine Servers unsupported
- moved web socket endpoint from `/api/host/stream` to `/api/host/stream/web_socket`
  - Change the Web Socket Endpoint when using a Reserve Proxy: See [Proxying via Apache2](#proxying-via-apache-2)
- `config.json` is not generated at first startup and can optionally be used for more granular control
  - generate the currently used config with `./web-server config generate`
- removed old unused `default_settings` value in the config

## Contributors
Thanks to everyone who contributed to make this software better :).

<a href = "https://github.com/Tanu-N-Prabhu/Python/graphs/contributors">
  <img src = "https://contrib.rocks/image?repo=MrCreativ3001/moonlight-web-stream"/>
</a>

## Building
Clone this repository:
```sh
git clone https://github.com/MrCreativ3001/moonlight-web-stream.git
```
A [Rust](https://www.rust-lang.org/tools/install) installation is required.

Moonlight Web consists the [web server binary](#building-the-web-server) and a [web frontend](#building-the-frontend):

### Building the Web Server
Run
```sh
cargo build --release
```
.

You can use [Cargo Cross](https://github.com/cross-rs/cross) for cross compilation:
```sh
cross build --release --target YOUR_TARGET
```
Note: windows only has the gnu target `x86_64-pc-windows-gnu`

### Building the Frontend
Build the web frontend with [npm](https://docs.npmjs.com/downloading-and-installing-node-js-and-npm).

To generate bindings to rust code it is using [Uniffi Bindgen React Native](https://github.com/jhugman/uniffi-bindgen-react-native).
This has a dependency on wasm-bindgen.

```sh
cargo install wasm-bindgen-cli
npm install
npm run build
```

The build output will be in `dist/`.

If you're compiling in:
- debug mode -> the folder needs to be called `dist/`
- release mode -> the folder needs to be called `static/`
