/** Thin wrapper around the Tauri IPC bridge.
 *
 * In the packaged app every call goes to the Rust core through Tauri.
 * When the UI runs in a plain browser (`npm run dev`) there is no Rust side,
 * so each module in this folder falls back to the local fixture set in
 * `mock.ts`. The fallback is dev-only scaffolding, not a product feature.
 */

type TauriGlobal = { __TAURI_INTERNALS__?: unknown };

export const isTauri =
  typeof window !== "undefined" && Boolean((window as TauriGlobal).__TAURI_INTERNALS__);

export async function invoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke: call } = await import("@tauri-apps/api/core");
  return call<T>(command, args);
}

export function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
