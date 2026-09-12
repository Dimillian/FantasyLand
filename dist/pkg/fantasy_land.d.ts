/* tslint:disable */
/* eslint-disable */

export class Game {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    static create(canvas: HTMLCanvasElement, seed: number): Promise<Game>;
    face(yaw: number, pitch: number): void;
    features(cx: number, cz: number, span: number): any;
    is_ready(): boolean;
    landscape_destinations(): any;
    look(dx: number, dy: number): void;
    map_data(cx: number, cz: number, span: number, res: number): Uint8Array;
    natural_destinations(): any;
    render_resolution(): Uint32Array;
    resize(width: number, height: number): void;
    return_to_spawn(): void;
    set_ascii(cell_scale: number, palette: number): void;
    set_enclosure(enabled: boolean): void;
    set_filter(mode: number, strength: number): void;
    set_ground_cover_density(density: number): void;
    set_quality(q: number): void;
    set_reflections(enabled: boolean): void;
    set_render_resolution(height: number): void;
    set_shadows(enabled: boolean): void;
    set_time(hour: number): void;
    set_weather_mode(mode: number): void;
    set_weather_paused(paused: boolean): void;
    set_weather_speed(speed: number): void;
    spawn(): Float32Array;
    state(): any;
    teleport(x: number, z: number): void;
    tick(dt: number, forward: number, strafe: number, sprint: boolean, jump: boolean): void;
    walking_journeys(): any;
    world_size(): number;
}

export function inspect_landscapes(seed: number): any;

export function inspect_map(seed: number, cx: number, cz: number, span: number, res: number): Uint8Array;

export function inspect_routes(seed: number, cx: number, cz: number, span: number): any;

export function inspect_world(seed: number, x: number, z: number): any;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_game_free: (a: number, b: number) => void;
    readonly game_create: (a: any, b: number) => any;
    readonly game_face: (a: number, b: number, c: number) => void;
    readonly game_features: (a: number, b: number, c: number, d: number) => any;
    readonly game_is_ready: (a: number) => number;
    readonly game_landscape_destinations: (a: number) => any;
    readonly game_look: (a: number, b: number, c: number) => void;
    readonly game_map_data: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly game_natural_destinations: (a: number) => any;
    readonly game_render_resolution: (a: number) => [number, number];
    readonly game_resize: (a: number, b: number, c: number) => void;
    readonly game_return_to_spawn: (a: number) => void;
    readonly game_set_ascii: (a: number, b: number, c: number) => void;
    readonly game_set_enclosure: (a: number, b: number) => void;
    readonly game_set_filter: (a: number, b: number, c: number) => void;
    readonly game_set_ground_cover_density: (a: number, b: number) => void;
    readonly game_set_quality: (a: number, b: number) => void;
    readonly game_set_reflections: (a: number, b: number) => void;
    readonly game_set_render_resolution: (a: number, b: number) => void;
    readonly game_set_shadows: (a: number, b: number) => void;
    readonly game_set_time: (a: number, b: number) => void;
    readonly game_set_weather_mode: (a: number, b: number) => void;
    readonly game_set_weather_paused: (a: number, b: number) => void;
    readonly game_set_weather_speed: (a: number, b: number) => void;
    readonly game_spawn: (a: number) => [number, number];
    readonly game_state: (a: number) => any;
    readonly game_teleport: (a: number, b: number, c: number) => void;
    readonly game_tick: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly game_walking_journeys: (a: number) => any;
    readonly game_world_size: (a: number) => number;
    readonly inspect_landscapes: (a: number) => any;
    readonly inspect_map: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly inspect_routes: (a: number, b: number, c: number, d: number) => any;
    readonly inspect_world: (a: number, b: number, c: number) => any;
    readonly wasm_bindgen_38dda96d1ba90cd1___convert__closures_____invoke___js_sys_4b348edf86b64934___Function_fn_wasm_bindgen_38dda96d1ba90cd1___JsValue_____wasm_bindgen_38dda96d1ba90cd1___sys__Undefined___js_sys_4b348edf86b64934___Function_fn_wasm_bindgen_38dda96d1ba90cd1___JsValue_____wasm_bindgen_38dda96d1ba90cd1___sys__Undefined_______true_: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen_38dda96d1ba90cd1___convert__closures_____invoke___wasm_bindgen_38dda96d1ba90cd1___JsValue__core_ed718c3d60ebd546___result__Result_____wasm_bindgen_38dda96d1ba90cd1___JsError___true_: (a: number, b: number, c: any) => [number, number];
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
