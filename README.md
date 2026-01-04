
# Moonlight Web
An unofficial [Moonlight Client](https://moonlight-stream.org/) allowing you to stream your pc to the Web.
It hosts a Web Server which will forward [Sunshine](https://docs.lizardbyte.dev/projects/sunshine/latest/) traffic to a Browser using the [WebRTC Api](https://webrtc.org/).

![An image displaying: PC with sunshine and moonlight web installed, a browser making requests to it](/readme/structure.png)

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
- [Config](#config)
- [Migrating to v2](#migrating-to-v2)
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

## Setup

1. Add a new user by typing in your name and password. The first user to login will be created and will be the admin.

2. Add a new pc (<img src="moonlight-web/web-server/web/resources/ic_add_to_queue_white_48px.svg" alt="icon" style="height:1em; vertical-align:middle;">) with the address as `localhost` and leave the port empty (if you've got the default port)

2. Pair your pc by clicking on the host (<img src="moonlight-web/web-server/web/resources/desktop_windows-48px.svg" alt="icon" style="height:1em; vertical-align:middle;">). Then enter the code in sunshine

3. Launch an app

### Streaming over the Internet

1. Forward the web server port on your router (default is 8080, http is 80, https is 443). You can see this in the config as the [`bind_address`](#bind-address)

When in a local network the WebRTC Peers will negotatiate without any problems.
When you want to play to over the Internet the STUN servers included by default will try to negotiate the peers directly.
This works for most of the networks, but if your network is very restrictive it might not work.
If this is the case try to configure one or both of these options:
1. The most reliable and recommended way is to use a [turn server](#configure-a-turn-server)
2. [Forward the ports directly](#port-forward) (this might not work if the firewall blocks udp)

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
                    "stun:stun.l.google.com:5349",
                    "stun:stun1.l.google.com:3478",
                    "stun:stun1.l.google.com:5349",
                    "stun:stun2.l.google.com:19302",
                    "stun:stun2.l.google.com:5349",
                    "stun:stun3.l.google.com:3478",
                    "stun:stun3.l.google.com:5349",
                    "stun:stun4.l.google.com:19302",
                    "stun:stun4.l.google.com:5349",
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

1. Set the port range used by the WebRTC Peer to a fixed range in the [config](#config)
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

You can generate a self signed certificate with this python script [moonlight-web/web-server/generate_certificate.py](moonlight-web/web-server/generate_certificate.py):

```sh
pip install pyOpenSSL
python ./moonlight-web/web-server/generate_certificate.py
```

2. Copy the files `server/key.pem` and `server/cert.pem` into your `server` directory.

3. Modify the [config](#config) to enable https using the certificates
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
<Location ${MOONLIGHT_SUBPATH}/api/host/stream>
        ProxyPass ws://${MOONLIGHT_STREAMER}${MOONLIGHT_SUBPATH}/api/host/stream
        ProxyPassReverse ws://${MOONLIGHT_STREAMER}${MOONLIGHT_SUBPATH}/api/host/stream
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

## Config
The config file is under `server/config.json` relative to the executable.
Here are the most important settings for configuring Moonlight Web.

Most options have command line arguments or environment variables associated with them.
```sh
./web-server help
```

For a full list of values look into the [Rust Config module](moonlight-web/common/src/config.rs).

### Bind Address 
The address and port the website will run on

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

### Default Settings
This will overwrite the default config of any new browser to open the website.

```jsonc
{
    "default_settings": {
        // possible values: "left", "right", "up", "down"
        "sidebarEdge": "left",
        "bitrate": 10000,
        "packetSize": 2048,
        "fps": 60,
        "videoFrameQueueSize": 3,
        // possible values: "720p", "1080p", "1440p", "4k", "native", "custom"
        "videoSize": "custom",
        // only works if videoSize=custom
        "videoSizeCustom": {
            "width": 1920,
            "height": 1080
        },
        // possible values: "h264", "h265", "av1", "auto"
        "videoCodec": "h264",
        "forceVideoElementRenderer": false,
        "canvasRenderer": false,
        "playAudioLocal": false,
        "audioSampleQueueSize": 20,
        // possible values: "highres", "normal"
        "mouseScrollMode": "highres",
        "controllerConfig": {
            "invertAB": false,
            "invertXY": false,
            // possible values: null or a number, example: 60, 120
            "sendIntervalOverride": null
        },
        // possible values: "auto", "webrtc", "websocket"
        "dataTransport": "auto",
        "toggleFullscreenWithKeybind": false,
        // possible values: "standard", "old"
        "pageStyle": "standard"
    }
}
```

### WebRTC Port Range
This will set the port range on the web server used to communicate when using WebRTC

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
                    "stun:stun.l.google.com:5349",
                    "stun:stun1.l.google.com:3478",
                    "stun:stun1.l.google.com:5349",
                    "stun:stun2.l.google.com:19302",
                    "stun:stun2.l.google.com:5349",
                    "stun:stun3.l.google.com:3478",
                    "stun:stun3.l.google.com:5349",
                    "stun:stun4.l.google.com:19302",
                    "stun:stun4.l.google.com:5349",
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

On first startup you can disable all default ice servers with the cli argument `--disable-default-webrtc-ice-servers` or the environment variable `DISABLE_DEFAULT_WEBRTC_ICE_SERVERS`.
After the `config.json` has been generated all ice server in it will be used, even if those are the defaults.

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

## Contributors
- Thanks to [@Argon2000](https://github.com/Argon2000) for implementing a canvas renderer, which makes this run in the Tesla browser.
- Thanks to [@Maneetbal](https://github.com/Maneetbal) for creating a new beautiful GUI.
- Thanks to [@chromaticpipe](https://github.com/chromaticpipe) for making Github CI.
- Thanks to [@qiin2333](https://github.com/qiin2333) for implementing HDR support.

## Building
Make sure you've cloned this repo with all it's submodules
```sh
git clone --recursive https://github.com/MrCreativ3001/moonlight-web-stream.git
```
A [Rust](https://www.rust-lang.org/tools/install) [nightly](https://rust-lang.github.io/rustup/concepts/channels.html) installation is required.

There are 2 ways to build Moonlight Web:
- Build it on your system

  When you want to build it on your system take a look at how to compile the crates:
  - [moonlight common sys](#crate-moonlight-common-sys)
  - [moonlight web server](#crate-moonlight-web-server)
  - [moonlight web streamer](#crate-moonlight-web-streamer)

- Compile using [Cargo Cross](https://github.com/cross-rs/cross)

  After you've got a successful installation of cross just run the command in the project root directory
  This will compile the [web server](#crate-moonlight-web-server) and the [streamer](#crate-moonlight-web-streamer)
  ```sh
  cross build --release --target YOUR_TARGET
  ```
  Note: windows only has the gnu target `x86_64-pc-windows-gnu`

### Crate: Moonlight Common Sys
[moonlight-common-sys](./moonlight-common-sys/) are rust bindings to the cpp [moonlight-common-c](https://github.com/moonlight-stream/moonlight-common-c) library.

Required for building:
- A [CMake installation](https://cmake.org/download/) which will automatically compile the [moonlight-common-c](https://github.com/moonlight-stream/moonlight-common-c) library
- [openssl-sys](https://docs.rs/openssl-sys/0.9.109/openssl_sys/): For information on building openssl sys go to the [openssl docs](https://docs.rs/openssl/latest/openssl/)
- A [bindgen installation](https://rust-lang.github.io/rust-bindgen/requirements.html) for generating the bindings to the [moonlight-common-c](https://github.com/moonlight-stream/moonlight-common-c) library

### Crate: Moonlight Web Server
This is the web server for Moonlight Web found at `moonlight-web/web-server/`.
It'll spawn a multiple [streamers](#crate-moonlight-web-server) as a subprocess for handling each stream.

Required for building:
- [moonlight-common-sys](#moonlight-common-sys)

Build the web frontend with [npm](https://docs.npmjs.com/downloading-and-installing-node-js-and-npm).
```sh
npm install
npm run build
```
The build output will be in `moonlight-web/web-server/dist`. The dist folder needs to be called `static` and in the same directory as the web server executable.

### Crate: Moonlight Web Streamer
This is the streamer subprocess of the [web server](#crate-moonlight-web-server) and found at `moonlight-web/streamer/`.
It'll communicate via stdin and stdout with the web server to negotiate the WebRTC peers and then continue to communicate via the peer.

Required for building:
- [moonlight-common-sys](#moonlight-common-sys)
