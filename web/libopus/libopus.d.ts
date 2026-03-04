// TypeScript bindings for emscripten-generated code.  Automatically generated at compile time.
declare namespace RuntimeExports {
    /**
     * @param {string=} returnType
     * @param {Array=} argTypes
     * @param {Object=} opts
     */
    function cwrap(ident: any, returnType?: string | undefined, argTypes?: any[] | undefined, opts?: any | undefined): (...args: any[]) => any;
    /**
     * @param {string|null=} returnType
     * @param {Array=} argTypes
     * @param {Array=} args
     * @param {Object=} opts
     */
    function ccall(ident: any, returnType?: (string | null) | undefined, argTypes?: any[] | undefined, args?: any[] | undefined, opts?: any | undefined): any;
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
    let HEAPF32: any;
}
interface WasmModule {
  _opus_multistream_decoder_create(_0: number, _1: number, _2: number, _3: number, _4: number, _5: number): number;
  _opus_multistream_decode_float(_0: number, _1: number, _2: number, _3: number, _4: number, _5: number): number;
  _opus_multistream_decoder_destroy(_0: number): void;
  _malloc(_0: number): number;
  _free(_0: number): void;
}

export type MainModule = WasmModule & typeof RuntimeExports;
export default function MainModuleFactory (options?: unknown): Promise<MainModule>;
