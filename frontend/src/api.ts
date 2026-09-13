import type { Target } from "./types";

const BASE = typeof window !== "undefined" ? window.location.origin : "http://localhost:8080";

async function fetchJson<T>(url: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${url}`, {
    headers: { "Content-Type": "application/json" },
    cache: "no-cache",
    ...init,
  });
  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new Error(`HTTP ${res.status}${body ? `: ${body}` : ""}`);
  }
  return res.json();
}

export const api = {
  async listTargets(): Promise<Target[]> {
    return fetchJson<Target[]>("/api/targets");
  },

  async scaleTarget(ns: string, name: string, replicas: number): Promise<Target> {
    return fetchJson<Target>(
      `/api/targets/${encodeURIComponent(ns)}/${encodeURIComponent(name)}/scale`,
      {
        method: "POST",
        body: JSON.stringify({ replicas }),
      },
    );
  },
};
