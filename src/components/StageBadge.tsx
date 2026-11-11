import { stageVar } from "../lib/format";
import type { Stage } from "../types";

/** Stage is the information that matters on push day: always a colored dot plus the word. */
export function StageBadge({ stage }: { stage: Stage }) {
  return (
    <span className="inline-flex items-center gap-1.5 text-[12px]" style={{ color: stageVar(stage) }}>
      <span aria-hidden className="inline-block h-2 w-2 rounded-full" style={{ background: stageVar(stage) }} />
      {stage}
    </span>
  );
}
