import { AssemblyAuthoringPanel } from "./assembly-authoring-panel";
import { CreatureCoherencePanel } from "./creature-coherence-panel";
import type { DomainPanelProps } from "./domain-authoring";
import { EnvironmentAuthoringPanel } from "./environment-authoring-panel";
import { GeologyAuthoringPanel } from "./geology-authoring-panel";
import { MaterialAuthoringPanel } from "./material-authoring-panel";
import { PerformanceAuthoringPanel } from "./performance-authoring-panel";
import { VegetationAuthoringPanel } from "./vegetation-authoring-panel";
import { WorldAuthoringPanel } from "./world-authoring-panel";

/** Keep domain-specific controls out of the Studio shell. */
export function DomainAuthoringPanels(props: DomainPanelProps) {
  const { document } = props;
  switch (document.kind) {
    case "object":
      return <AssemblyAuthoringPanel {...props} document={document} />;
    case "character":
      return (
        <>
          <CreatureCoherencePanel {...props} document={document} />
          <PerformanceAuthoringPanel {...props} document={document} />
        </>
      );
    case "vegetation":
      return <VegetationAuthoringPanel {...props} document={document} />;
    case "terrain":
      return <GeologyAuthoringPanel {...props} document={document} />;
    case "material":
      return <MaterialAuthoringPanel {...props} document={document} />;
    case "world":
      return <WorldAuthoringPanel {...props} document={document} />;
    case "environment":
    case "lighting":
    case "water":
    case "stage":
      return <EnvironmentAuthoringPanel {...props} document={document} />;
  }
}
