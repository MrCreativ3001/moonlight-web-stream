import { StreamPermissions } from "../../api_bindings.js";
import { Component } from "../index.js";
import { InputComponent } from "../input.js";

export function defaultRolePermissions(): StreamPermissions {
    return {
        allow_add_hosts: true,
        maximum_bitrate_kbps: null,
        allow_codec_h264: true,
        allow_codec_h265: true,
        allow_codec_av1: true,
        allow_hdr: true,
        allow_transport_webrtc: true,
        allow_transport_websockets: true,
    }
}

export class RolePermissionsMenu implements Component {

    private rootDiv = document.createElement("div")

    private allowAddHosts: InputComponent
    private maximumBitrateKbps: InputComponent
    private allowCodecH264: InputComponent
    private allowCodecH265: InputComponent
    private allowCodecAv1: InputComponent
    private allowHdr: InputComponent
    private allowWebRTC: InputComponent
    private allowWebSockets: InputComponent

    constructor(permissions: StreamPermissions = defaultRolePermissions()) {

        // Allow Add Hosts
        this.allowAddHosts = new InputComponent("allowAddHosts", "checkbox", "Allow adding Hosts", {
            checked: permissions.allow_add_hosts,
        })
        this.allowAddHosts.mount(this.rootDiv)

        // Maximum Bitrate
        this.maximumBitrateKbps = new InputComponent("maximumBitrateKpbs", "number", "Maximum Bitrate", {
            hasEnableCheckbox: true,
            defaultValue: `${permissions.maximum_bitrate_kbps ?? 10000}`,
            step: "100",
            numberSlider: {
                range_min: 1000,
                range_max: 10000,
            }
        })
        this.maximumBitrateKbps.setEnabled(permissions.maximum_bitrate_kbps != null)
        this.maximumBitrateKbps.mount(this.rootDiv)

        // Codecs
        this.allowCodecH264 = new InputComponent("allowCodecH264", "checkbox", "Allow H264", {
            checked: permissions.allow_codec_h264,
        })
        this.allowCodecH264.mount(this.rootDiv)

        this.allowCodecH265 = new InputComponent("allowCodecH265", "checkbox", "Allow H265", {
            checked: permissions.allow_codec_h265,
        })
        this.allowCodecH265.mount(this.rootDiv)

        this.allowCodecAv1 = new InputComponent("allowCodecAv1", "checkbox", "Allow Av1", {
            checked: permissions.allow_codec_av1,
        })
        this.allowCodecAv1.mount(this.rootDiv)

        // Hdr
        this.allowHdr = new InputComponent("allowHdr", "checkbox", "Allow HDR", {
            checked: permissions.allow_hdr,
        })
        this.allowHdr.mount(this.rootDiv)

        // Transport
        this.allowWebRTC = new InputComponent("allowTransportWebRTC", "checkbox", "Allow WebRTC", {
            checked: permissions.allow_transport_webrtc,
        })
        this.allowWebRTC.mount(this.rootDiv)

        this.allowWebSockets = new InputComponent("allowTransportWebSockets", "checkbox", "Allow Web Sockets", {
            checked: permissions.allow_transport_websockets,
        })
        this.allowWebSockets.mount(this.rootDiv)
    }

    getPermissions(): StreamPermissions {
        return {
            allow_add_hosts: this.allowAddHosts.isChecked(),
            maximum_bitrate_kbps: this.maximumBitrateKbps.isEnabled() ? parseInt(this.maximumBitrateKbps.getValue()) : null,
            allow_codec_h264: this.allowCodecH264.isChecked(),
            allow_codec_h265: this.allowCodecH265.isChecked(),
            allow_codec_av1: this.allowCodecAv1.isChecked(),
            allow_hdr: this.allowHdr.isChecked(),
            allow_transport_webrtc: this.allowWebRTC.isChecked(),
            allow_transport_websockets: this.allowWebSockets.isChecked(),
        }
    }

    mount(parent: HTMLElement): void {
        parent.appendChild(this.rootDiv)
    }
    unmount(parent: HTMLElement): void {
        parent.removeChild(this.rootDiv)
    }
}