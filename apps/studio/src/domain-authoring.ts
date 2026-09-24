import type { Operation } from "@wrela/authoring";
import type { Document, Project } from "@wrela/model";

/** Domain editors share transactions and preview services, never runtime ownership. */
export type DomainPanelProps<T extends Document = Document> = {
  document: T;
  project: Project;
  onChange: (path: (string | number)[], value: unknown, label?: string) => void;
  onApply: (operations: Operation[], label: string) => void;
  onSeek?: (seconds: number) => void;
  onSeekMotion?: (motion: string, seconds: number) => void;
  onTransition?: (from: string, to: string, at: number, elapsed: number, playing?: boolean) => void;
  onPlay?: (motion?: string) => void;
  onReview?: (scenarioId: string, cameraId: string, mode: "lit" | "silhouette") => void;
  onPreview?: (options: {
    distance?: number;
    sunElevation?: number;
    sunAzimuth?: number;
    mode?: "lit" | "silhouette";
    time?: number;
  }) => void;
};
