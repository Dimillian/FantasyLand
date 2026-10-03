/* tslint:disable */
/* eslint-disable */

export class Game {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    accept_gi_result(ticket: number, bytes: Uint8Array): boolean;
    accept_stream_result(ticket: number, bytes: Uint8Array): boolean;
    /**
     * An asynchronous queue barrier for repeatable benchmark setup. Rendering
     * remains nonblocking; the browser waits on RAF while the callback fires.
     */
    begin_gpu_drain(): void;
    character_state(): any;
    combat_events(): any;
    combat_frame(): Float32Array;
    combat_input(attack: boolean, block: boolean, paused: boolean): void;
    corpse_loot(): any;
    static create(canvas: HTMLCanvasElement, seed: number): Promise<Game>;
    dialogue(topic: string): any;
    encounter_locations(): any;
    end_dialogue(): void;
    equip_item(id: string, on: boolean): boolean;
    face(yaw: number, pitch: number): void;
    features(cx: number, cz: number, span: number): any;
    gpu_drained(): boolean;
    interact(): any;
    interaction_label(): string;
    is_ready(): boolean;
    landscape_destinations(): any;
    /**
     * Small audio packet matching the weather/event uniforms of the rendered
     * frame. Avoids repeating full world/acoustic probes at frame frequency.
     */
    lightning_audio_frame(): Float32Array;
    look(dx: number, dy: number): void;
    map_data(cx: number, cz: number, span: number, res: number): Uint8Array;
    map_layer_data(cx: number, cz: number, span: number, res: number, layer: number): Uint8Array;
    natural_destinations(): any;
    next_gi_job(): any;
    next_stream_job(): Int32Array;
    pending_chunks(): number;
    render_resolution(): Uint32Array;
    resize(width: number, height: number): void;
    restore_clock(clock: number): void;
    restore_snapshot(value: any): void;
    return_to_spawn(): void;
    revive(): void;
    save_snapshot(): any;
    set_antialiasing(mode: number): void;
    set_async_streaming(enabled: boolean): void;
    set_enclosure(enabled: boolean): void;
    set_filter(mode: number, strength: number): void;
    set_ground_cover_density(density: number): void;
    set_lighting_mode(mask: number): void;
    set_meadow(enabled: boolean): void;
    set_quality(q: number): void;
    set_reflections(enabled: boolean): void;
    set_render_resolution(height: number): void;
    set_shadows(enabled: boolean): void;
    set_time(hour: number): void;
    set_weather_mode(mode: number): void;
    set_weather_paused(paused: boolean): void;
    set_weather_speed(speed: number): void;
    settlement_destinations(): any;
    settlement_inspect(id: number): any;
    spawn(): Float32Array;
    start_combat(): boolean;
    start_combat_kind(kind: number): boolean;
    state(): any;
    stop_combat(): void;
    take_loot(corpse_id: string, item_id: string): boolean;
    teleport(x: number, z: number): void;
    tick(dt: number, forward: number, strafe: number, sprint: boolean, jump: boolean): void;
    toggle_weapon(): void;
    visit_encounter(x: number, z: number): boolean;
    /**
     * Art review visits actual procedural rooms in the current seeded world.
     */
    visit_interior(kind: string): string;
    visit_settlement(id: number): void;
    walking_journeys(): any;
    world_identity(): any;
    world_size(): number;
}

export class StreamGenerator {
    free(): void;
    [Symbol.dispose](): void;
    generate(kind: number, x: number, z: number, lod: number, detail: number): Uint8Array;
    /**
     * Separate worker packet: a bounded local geometry proxy for diffuse GI.
     * IDs remain u32 throughout the JS bridge (f32 would lose high ID bits).
     */
    generate_gi(origin_x: number, origin_y: number, origin_z: number, door_ids: Uint32Array, door_angles: Float32Array): Uint8Array;
    map_features(x: number, z: number, span: number): any;
    map_layer_data(x: number, z: number, span: number, res: number, layer: number): Uint8Array;
    constructor(seed: number);
    world_identity(): any;
}

export function inspect_landscapes(seed: number): any;

export function inspect_map(seed: number, cx: number, cz: number, span: number, res: number): Uint8Array;

export function inspect_routes(seed: number, cx: number, cz: number, span: number): any;

export function inspect_world(seed: number, x: number, z: number): any;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_game_free: (a: number, b: number) => void;
    readonly __wbg_streamgenerator_free: (a: number, b: number) => void;
    readonly game_accept_gi_result: (a: number, b: number, c: number, d: number) => number;
    readonly game_accept_stream_result: (a: number, b: number, c: number, d: number) => number;
    readonly game_begin_gpu_drain: (a: number) => void;
    readonly game_character_state: (a: number) => any;
    readonly game_combat_events: (a: number) => any;
    readonly game_combat_frame: (a: number) => [number, number];
    readonly game_combat_input: (a: number, b: number, c: number, d: number) => void;
    readonly game_corpse_loot: (a: number) => any;
    readonly game_create: (a: any, b: number) => any;
    readonly game_dialogue: (a: number, b: number, c: number) => any;
    readonly game_encounter_locations: (a: number) => any;
    readonly game_end_dialogue: (a: number) => void;
    readonly game_equip_item: (a: number, b: number, c: number, d: number) => number;
    readonly game_face: (a: number, b: number, c: number) => void;
    readonly game_features: (a: number, b: number, c: number, d: number) => any;
    readonly game_gpu_drained: (a: number) => number;
    readonly game_interact: (a: number) => any;
    readonly game_interaction_label: (a: number) => [number, number];
    readonly game_is_ready: (a: number) => number;
    readonly game_landscape_destinations: (a: number) => any;
    readonly game_lightning_audio_frame: (a: number) => [number, number];
    readonly game_look: (a: number, b: number, c: number) => void;
    readonly game_map_data: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly game_map_layer_data: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly game_natural_destinations: (a: number) => any;
    readonly game_next_gi_job: (a: number) => any;
    readonly game_next_stream_job: (a: number) => [number, number];
    readonly game_pending_chunks: (a: number) => number;
    readonly game_render_resolution: (a: number) => [number, number];
    readonly game_resize: (a: number, b: number, c: number) => void;
    readonly game_restore_clock: (a: number, b: number) => void;
    readonly game_restore_snapshot: (a: number, b: any) => [number, number];
    readonly game_return_to_spawn: (a: number) => void;
    readonly game_revive: (a: number) => void;
    readonly game_save_snapshot: (a: number) => any;
    readonly game_set_antialiasing: (a: number, b: number) => void;
    readonly game_set_async_streaming: (a: number, b: number) => void;
    readonly game_set_enclosure: (a: number, b: number) => void;
    readonly game_set_filter: (a: number, b: number, c: number) => void;
    readonly game_set_ground_cover_density: (a: number, b: number) => void;
    readonly game_set_lighting_mode: (a: number, b: number) => void;
    readonly game_set_meadow: (a: number, b: number) => void;
    readonly game_set_quality: (a: number, b: number) => void;
    readonly game_set_reflections: (a: number, b: number) => void;
    readonly game_set_render_resolution: (a: number, b: number) => void;
    readonly game_set_shadows: (a: number, b: number) => void;
    readonly game_set_time: (a: number, b: number) => void;
    readonly game_set_weather_mode: (a: number, b: number) => void;
    readonly game_set_weather_paused: (a: number, b: number) => void;
    readonly game_set_weather_speed: (a: number, b: number) => void;
    readonly game_settlement_destinations: (a: number) => any;
    readonly game_settlement_inspect: (a: number, b: number) => any;
    readonly game_spawn: (a: number) => [number, number];
    readonly game_start_combat: (a: number) => number;
    readonly game_start_combat_kind: (a: number, b: number) => number;
    readonly game_state: (a: number) => any;
    readonly game_stop_combat: (a: number) => void;
    readonly game_take_loot: (a: number, b: number, c: number, d: number, e: number) => number;
    readonly game_teleport: (a: number, b: number, c: number) => void;
    readonly game_tick: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly game_toggle_weapon: (a: number) => void;
    readonly game_visit_encounter: (a: number, b: number, c: number) => number;
    readonly game_visit_interior: (a: number, b: number, c: number) => [number, number];
    readonly game_visit_settlement: (a: number, b: number) => void;
    readonly game_walking_journeys: (a: number) => any;
    readonly game_world_identity: (a: number) => any;
    readonly game_world_size: (a: number) => number;
    readonly inspect_landscapes: (a: number) => any;
    readonly inspect_map: (a: number, b: number, c: number, d: number, e: number) => [number, number];
    readonly inspect_routes: (a: number, b: number, c: number, d: number) => any;
    readonly inspect_world: (a: number, b: number, c: number) => any;
    readonly streamgenerator_generate: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly streamgenerator_generate_gi: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => [number, number];
    readonly streamgenerator_map_features: (a: number, b: number, c: number, d: number) => any;
    readonly streamgenerator_map_layer_data: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
    readonly streamgenerator_new: (a: number) => number;
    readonly streamgenerator_world_identity: (a: number) => any;
    readonly wasm_bindgen_38dda96d1ba90cd1___convert__closures_____invoke___js_sys_4b348edf86b64934___Function_fn_wasm_bindgen_38dda96d1ba90cd1___JsValue_____wasm_bindgen_38dda96d1ba90cd1___sys__Undefined___js_sys_4b348edf86b64934___Function_fn_wasm_bindgen_38dda96d1ba90cd1___JsValue_____wasm_bindgen_38dda96d1ba90cd1___sys__Undefined_______true_: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen_38dda96d1ba90cd1___convert__closures_____invoke___wasm_bindgen_38dda96d1ba90cd1___JsValue__core_ed718c3d60ebd546___result__Result_____wasm_bindgen_38dda96d1ba90cd1___JsError___true_: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_38dda96d1ba90cd1___convert__closures_____invoke___wasm_bindgen_38dda96d1ba90cd1___JsValue______true_: (a: number, b: number, c: any) => void;
    readonly wasm_bindgen_38dda96d1ba90cd1___convert__closures_____invoke___wasm_bindgen_38dda96d1ba90cd1___JsValue______true__80: (a: number, b: number, c: any) => void;
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
