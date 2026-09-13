import { useEffect, useState, useRef } from "react";
import { api } from "@/api";
import type { Target } from "@/types";
import { TargetsTable } from "@/components/targets-table";
import { Button } from "@/components/ui/button";
import { RefreshCw } from "lucide-react";

const REFRESH_INTERVAL_MS = 10_000;

export default function App() {
  const [targets, setTargets] = useState<Target[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const loadingRef = useRef(false);

  const loadTargets = async (silent = false) => {
    if (loadingRef.current) return;
    loadingRef.current = true;
    if (!silent) setLoading(true);
    try {
      const t = await api.listTargets();
      setTargets(t);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      loadingRef.current = false;
      setLoading(false);
    }
  };

  useEffect(() => {
    loadTargets();
    const id = setInterval(() => loadTargets(true), REFRESH_INTERVAL_MS);
    return () => clearInterval(id);
  }, []);

  const handleScale = async (t: Target, replicas: number) => {
    setError(null);
    try {
      const updated = await api.scaleTarget(t.namespace, t.name, replicas);
      setTargets((prev) =>
        prev.map((x) =>
          x.namespace === updated.namespace && x.name === updated.name ? updated : x,
        ),
      );
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      loadTargets(true);
    }
  };

  return (
    <div className="min-h-screen bg-background">
      <header className="border-b px-6 py-4">
        <div className="flex items-center justify-between">
          <h1 className="text-xl font-bold text-primary">NVidia Replica Scaler WebUI</h1>
          <Button variant="outline" size="sm" onClick={() => loadTargets()}>
            <RefreshCw className={`mr-1 h-3 w-3 ${loading ? "animate-spin" : ""}`} />
            Refresh
          </Button>
        </div>
      </header>

      {error && (
        <div className="px-6 pt-4">
          <div className="rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
            {error}
          </div>
        </div>
      )}

      <main className="p-6">
        <p className="mb-4 text-sm text-muted-foreground">
          Start (1 replica) and stop (0 replicas) GPU workloads.
        </p>
        <TargetsTable targets={targets} onScale={handleScale} />
      </main>
    </div>
  );
}
