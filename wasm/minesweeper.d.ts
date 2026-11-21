/* tslint:disable */
/* eslint-disable */

export enum GameStatus {
    Ready = 0,
    Playing = 1,
    Won = 2,
    Lost = 3,
}

export class Minesweeper {
    free(): void;
    [Symbol.dispose](): void;
    cell(x: number, y: number): number;
    chord(x: number, y: number): number;
    flags_placed(): number;
    height(): number;
    mines(): number;
    mines_left(): number;
    constructor(width: number, height: number, mines: number, seed: number);
    reset(seed: number): void;
    reveal(x: number, y: number): number;
    seed(): number;
    status(): GameStatus;
    toggle_flag(x: number, y: number): boolean;
    view(): Uint8Array;
    width(): number;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_minesweeper_free: (a: number, b: number) => void;
    readonly minesweeper_cell: (a: number, b: number, c: number) => number;
    readonly minesweeper_chord: (a: number, b: number, c: number) => number;
    readonly minesweeper_flags_placed: (a: number) => number;
    readonly minesweeper_height: (a: number) => number;
    readonly minesweeper_mines: (a: number) => number;
    readonly minesweeper_mines_left: (a: number) => number;
    readonly minesweeper_new: (a: number, b: number, c: number, d: number) => number;
    readonly minesweeper_reset: (a: number, b: number) => void;
    readonly minesweeper_reveal: (a: number, b: number, c: number) => number;
    readonly minesweeper_seed: (a: number) => number;
    readonly minesweeper_status: (a: number) => number;
    readonly minesweeper_toggle_flag: (a: number, b: number, c: number) => number;
    readonly minesweeper_view: (a: number) => [number, number];
    readonly minesweeper_width: (a: number) => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
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
