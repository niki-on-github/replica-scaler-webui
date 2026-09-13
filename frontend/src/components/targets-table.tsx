import type { Target, TargetState } from "@/types";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Power, PowerOff } from "lucide-react";

function StateBadge({ state, desired }: { state: TargetState; desired: number | null }) {
  if (desired === 0) {
    return <Badge variant="secondary">stopped</Badge>;
  }
  if (desired === 1 || state === "running") {
    return <Badge variant="success">running</Badge>;
  }
  if (state === "pending") {
    return <Badge variant="outline">pending</Badge>;
  }
  return <Badge variant="outline">{state}</Badge>;
}

export function TargetsTable({
  targets,
  onScale,
}: {
  targets: Target[];
  onScale: (t: Target, replicas: number) => void;
}) {
  if (targets.length === 0) {
    return (
      <div className="rounded-lg border bg-card text-card-foreground p-8 text-center text-sm text-muted-foreground">
        No GPU workloads discovered.
      </div>
    );
  }

  return (
    <div className="overflow-x-auto rounded-lg border bg-card text-card-foreground">
      <table className="w-full text-sm">
        <thead>
          <tr className="border-b text-left text-muted-foreground">
            <th className="px-4 py-2 font-medium">Name</th>
            <th className="px-4 py-2 font-medium">Namespace</th>
            <th className="px-4 py-2 font-medium">Kind</th>
            <th className="px-4 py-2 font-medium">State</th>
            <th className="px-4 py-2 font-medium">Replicas</th>
            <th className="px-4 py-2 text-right font-medium">Actions</th>
          </tr>
        </thead>
        <tbody>
          {targets.map((t) => {
            const running = (t.replicas ?? 0) > 0;
            return (
              <tr key={`${t.kind}/${t.namespace}/${t.name}`} className="border-b last:border-0">
                <td className="px-4 py-2 font-medium">{t.name}</td>
                <td className="px-4 py-2">{t.namespace}</td>
                <td className="px-4 py-2">{t.kind}</td>
                <td className="px-4 py-2">
                  <StateBadge state={t.state} desired={t.desired_replicas} />
                </td>
                <td className="px-4 py-2">
                  {t.desired_replicas ?? t.replicas ?? 0}
                </td>
                <td className="px-4 py-2 text-right">
                  {!running ? (
                    <Button size="sm" onClick={() => onScale(t, 1)}>
                      <Power className="mr-1 h-3 w-3" />
                      Start
                    </Button>
                  ) : (
                    <Button size="sm" variant="destructive" onClick={() => onScale(t, 0)}>
                      <PowerOff className="mr-1 h-3 w-3" />
                      Stop
                    </Button>
                  )}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}
