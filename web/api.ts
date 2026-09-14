import { App, DeleteHostQuery, DetailedHost, GetAppImageQuery, GetAppsQuery, GetAppsResponse, GetHostQuery, GetHostResponse, GetHostsResponse, PostCancelRequest, PostCancelResponse, PostPairRequest, PostPairResponse1, PostPairResponse2, PostWakeUpRequest, PostHostRequest, PostHostResponse, UndetailedHost, PatchHostRequest, } from "./api_bindings"
import { buildUrl } from "./config_"
import { WebRtcLinkHeader_Tags, webrtcLinkHeaderParse } from "./uniffi/moonlight_common_bindings"

// IMPORTANT: this should be a bit bigger than the moonlight-common reqwest backend timeout if some hosts are offline!
const API_TIMEOUT = 12000

export function getApi(): Api {
    return {
        host_url: buildUrl("/api"),
    }
}

const OPTIONS = "OPTIONS"
const GET = "GET"
const POST = "POST"
const PATCH = "PATCH"

export type Api = {
    host_url: string
}

export type ApiFetchInit = {
    noUrlModify?: boolean,
    query?: any,
    noTimeout?: boolean,
    keepalive?: boolean,
} & (
        { json?: any, }
        | { sdp?: string }
        | { trickleIceSdpFrag?: string }
    )

export function isDetailedHost(host: UndetailedHost | DetailedHost): host is DetailedHost {
    return (host as DetailedHost).https_port !== undefined
}

function buildRequest(api: Api, endpoint: string, method: string, init?: ApiFetchInit): [string, RequestInit] {
    const queryObj = init?.query || {};
    const queryParts = [];
    for (const key in queryObj) {
        // Remove all null values from query, these cause problems in rust
        if (queryObj[key] != null) {
            queryParts.push(
                encodeURIComponent(key) + "=" + encodeURIComponent(queryObj[key])
            );
        }
    }
    const queryString = queryParts.length > 0 ? "?" + queryParts.join("&") : "";

    let url
    if (init?.noUrlModify) {
        url = `${endpoint}${queryString}`
    } else {
        url = `${api.host_url}${endpoint}${queryString}`
    }

    const headers: any = {};

    let body = null
    if (init) {
        if ("json" in init) {
            headers["Content-Type"] = "application/json"
            body = JSON.stringify(init.json)
        } else if ("sdp" in init) {
            headers["Content-Type"] = "application/sdp"
            body = init.sdp
        } else if ("trickleIceSdpFrag" in init) {
            headers["Content-Type"] = "application/trickle-ice-sdpfrag"
            body = init.trickleIceSdpFrag
        }
    }

    const request: RequestInit = {
        method: method,
        headers,
        body,
        credentials: "include"
    }

    if (init?.keepalive) {
        request.keepalive = true
    }

    return [url, request]
}

export class FetchError extends Error {
    private response?: Response

    constructor(type: "timeout", endpoint: string, method: string)
    constructor(type: "failed", endpoint: string, method: string, response: Response, reason?: string)
    constructor(type: "unknown", endpoint: string, method: string, error: Error)

    constructor(type: "timeout" | "failed" | "unknown", endpoint: string, method: string, responseOrError?: Response | any, reason?: string) {
        if (type == "timeout") {
            super(`failed to fetch ${method} at ${endpoint} because of timeout`)
        } else if (type == "failed") {
            const response = responseOrError as Response
            super(`failed to fetch ${method} at ${endpoint} with code ${response?.status} ${reason ? `because of ${reason}` : ""}`)

            this.response = response
        } else if (type == "unknown") {
            const error = responseOrError as Error
            super(`failed to fetch ${method} at ${endpoint} because of ${error}`)
        }
    }

    getResponse(): Response | null {
        return this.response ?? null
    }
}

class StreamedJsonResponse<Initial, Other> {
    response: Initial

    private reader
    private decoder = new TextDecoder()
    private bufferedText = ""

    constructor(body: ReadableStreamDefaultReader, response: Initial) {
        this.reader = body
        this.response = response
    }

    async next(): Promise<Other | null> {
        while (true) {
            const { done, value } = await this.reader.read()

            if (done) {
                return null
            }

            this.bufferedText += this.decoder.decode(value)

            const split = this.bufferedText.split("\n", 2)
            if (split.length == 2) {
                this.bufferedText = split[1]

                const text = split[0]
                const json = JSON.parse(text)

                return json
            }
        }
    }
}

export async function fetchApi(api: Api, endpoint: string, method: string, init?: { response?: "json" } & ApiFetchInit, timeout?: number): Promise<any>
export async function fetchApi(api: Api, endpoint: string, method: string, init: { response: "ignore" } & ApiFetchInit, timeout?: number): Promise<Response>
export async function fetchApi<Initial, Other>(api: Api, endpoint: string, method: string, init: { response: "jsonStreaming" } & ApiFetchInit, timeout?: number): Promise<StreamedJsonResponse<Initial, Other>>

export async function fetchApi(api: Api, endpoint: string, method: string = GET, init?: { response?: "json" | "ignore" | "jsonStreaming" } & ApiFetchInit, timeout: number = API_TIMEOUT) {
    const [url, request] = buildRequest(api, endpoint, method, init)

    if (!init?.noTimeout) {
        request.signal = AbortSignal.timeout(timeout)
    }

    let response
    try {
        response = await fetch(url, request)
    } catch (e: any) {
        throw new FetchError("unknown", endpoint, method, e)
    }

    if (!response.ok) {
        throw new FetchError("failed", endpoint, method, response)
    }

    if (init?.response == "ignore") {
        return response
    }

    if (init?.response == undefined || init.response == "json") {
        const json = await response.json()

        return json
    } else if (init?.response == "jsonStreaming") {
        if (!response.body) {
            throw new FetchError("failed", endpoint, method, response)
        }

        // @ts-ignore
        const stream = new StreamedJsonResponse(response.body?.getReader())
        const data = await stream.next()
        stream.response = data

        return stream
    }
}

export async function apiGetHosts(api: Api): Promise<StreamedJsonResponse<GetHostsResponse, UndetailedHost>> {
    return await fetchApi<GetHostsResponse, UndetailedHost>(api, "/hosts", GET, { response: "jsonStreaming" })
}
export async function apiGetHost(api: Api, query: GetHostQuery): Promise<DetailedHost> {
    const response = await fetchApi(api, "/host", GET, { query })

    return (response as GetHostResponse).host
}
export async function apiPostHost(api: Api, data: PostHostRequest): Promise<DetailedHost> {
    const response = await fetchApi(api, "/host", "post", { json: data })

    return (response as PostHostResponse).host
}
export async function apiPatchHost(api: Api, data: PatchHostRequest): Promise<void> {
    await fetchApi(api, "/host", PATCH, {
        json: data,
        response: "ignore"
    })
}
export async function apiDeleteHost(api: Api, query: DeleteHostQuery): Promise<void> {
    await fetchApi(api, "/host", "delete", { query, response: "ignore" })
}

export async function apiPostPair(api: Api, request: PostPairRequest): Promise<StreamedJsonResponse<PostPairResponse1, PostPairResponse2>> {
    return await fetchApi(api, "/pair", "post", {
        json: request,
        response: "jsonStreaming",
        noTimeout: true
    })
}

export async function apiWakeUp(api: Api, request: PostWakeUpRequest): Promise<void> {
    await fetchApi(api, "/host/wake", "post", {
        json: request,
        response: "ignore"
    })
}

export async function apiGetApps(api: Api, query: GetAppsQuery): Promise<Array<App>> {
    const response = await fetchApi(api, "/apps", GET, { query }) as GetAppsResponse

    return response.apps
}

export async function apiGetAppImage(api: Api, query: GetAppImageQuery): Promise<Blob> {
    const response = await fetchApi(api, "/app/image", GET, {
        query,
        response: "ignore"
    },
        60000)

    return await response.blob()
}

export async function apiHostCancel(api: Api, request: PostCancelRequest): Promise<PostCancelResponse> {
    const response = await fetchApi(api, "/host/cancel", POST, {
        json: request
    })

    return response as PostCancelResponse
}

export type WebRTCConfiguration = {
    iceServers: Array<RTCIceServer>
}

export async function apiWebRTCConfiguration(api: Api): Promise<WebRTCConfiguration> {
    const ENDPOINT = "/host/stream/webrtc"

    const [url, request] = buildRequest(api, ENDPOINT, OPTIONS)

    let response
    try {
        response = await fetch(url, request)
    } catch (e: any) {
        throw new FetchError("unknown", ENDPOINT, OPTIONS, e)
    }

    const iceServers: Array<RTCIceServer> = []

    const rawLinks = response.headers.get("Link")
    if (rawLinks) {
        const links = webrtcLinkHeaderParse(rawLinks)
        for (const link of links) {
            if (link.tag == WebRtcLinkHeader_Tags.IceServer) {
                iceServers.push({
                    urls: link.inner.url,
                    username: link.inner.username,
                    credential: link.inner.credential,
                })
            }
        }
    }

    return {
        iceServers
    }
}

export type WebRTCAnswer = {
    answerSdp: string,
    location: string | null,
}

export async function apiWebRTCOffer(api: Api, offerSdp: string): Promise<WebRTCAnswer> {
    const ENDPOINT = "/host/stream/webrtc"

    const [url, request] = buildRequest(api, ENDPOINT, POST, { sdp: offerSdp })

    let response
    try {
        response = await fetch(url, request)
    } catch (e: any) {
        throw new FetchError("unknown", ENDPOINT, POST, e)
    }

    // 201 == Created
    if (response.status != 201) {
        const reason = await response.text()
        throw new FetchError("failed", ENDPOINT, POST, response, reason)
    }

    // Get sdp
    const answerSdp = await response.text()

    // get location, if set
    let location = null
    for (const [name, value] of response.headers) {
        if (name.trim().toLowerCase() == "location") {
            location = value
        }
    }

    return {
        answerSdp,
        location,
    }
}
