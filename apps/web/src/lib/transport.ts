/**
 * Transport adapter. In a browser, requests go over HTTP to the Tendly server.
 * Inside the native (Tauri) shell they are dispatched in-process to the same
 * Rust router through a single `api_request` command, so the native app needs
 * no local network port, Node server or Docker.
 */

export type RawResponse = { status: number; headers: Record<string, string>; body: string };

type TauriInternals = { invoke?: unknown };

export function isNative(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window && !!(window as unknown as { __TAURI_INTERNALS__: TauriInternals }).__TAURI_INTERNALS__;
}

export async function send(method: string, path: string, headers: Record<string, string>, body?: string): Promise<RawResponse> {
  if (isNative()) {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<RawResponse>("api_request", { request: { method, path, headers, body: body ?? null } });
  }
  const res = await fetch(path, { method, headers, body, credentials: "same-origin" });
  const out: Record<string, string> = {};
  res.headers.forEach((v, k) => {
    out[k] = v;
  });
  return { status: res.status, headers: out, body: await res.text() };
}
