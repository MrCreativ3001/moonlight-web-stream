import { TinyH264Module } from "./TinyH264"

export type OnPictureReady = (buffer: Uint8Array, width: number, height: number) => void

export default class TinyH264Decoder {
    static RDY: number
    static PIC_RDY: number
    static HDRS_RDY: number
    static ERROR: number
    static PARAM_SET_ERROR: number
    static MEMALLOC_ERROR: number

    constructor(module: TinyH264Module, onPictureReady: OnPictureReady)

    decode(nal: ArrayBuffer | Uint8Array): void

    release(): void
}