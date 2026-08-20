#!/usr/bin/env node
/** Stdio host. Speaks the Rust/TS golden protocol. */
declare class BridgeSession {
    #private;
    handle(line: string): Promise<string>;
}
export declare function handle(line: string): Promise<string>;
export { BridgeSession };
