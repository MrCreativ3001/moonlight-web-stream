// TypeScript bindings for emscripten-generated code.  Automatically generated at compile time.
declare namespace RuntimeExports {
    function stackAlloc(sz: any): any;
    function stackSave(): any;
    function stackRestore(val: any): any;
    /**
     * @param {number} ptr
     * @param {number} value
     * @param {string} type
     */
    function setValue(ptr: number, value: number, type?: string): void;
    /**
     * @param {number} ptr
     * @param {string} type
     */
    function getValue(ptr: number, type?: string): any;
    function writeArrayToMemory(array: any, buffer: any): void;
    let HEAPU8: any;
    let HEAPU32: any;
}
interface WasmModule {
  _openh264_decoder_create(_0: number): number;
  _openh264_decoder_decode(_0: number, _1: number, _2: number, _3: number, _4: number, _5: number, _6: number, _7: number): number;
  _openh264_decoder_destroy(_0: number): void;
  _malloc(_0: number): number;
  _free(_0: number): void;
}

export type MainModule = WasmModule & typeof RuntimeExports;
export default function MainModuleFactory (options?: unknown): Promise<MainModule>;
