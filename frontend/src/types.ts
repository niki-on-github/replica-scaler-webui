export type TargetState = "running" | "stopped" | "pending";

export interface Target {
  kind: "Deployment" | "StatefulSet";
  name: string;
  namespace: string;
  replicas: number | null;
  ready_replicas: number | null;
  available_replicas: number | null;
  desired_replicas: number | null;
  gpu: boolean;
  match_reason: string;
  state: TargetState;
}
