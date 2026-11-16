import type { ActivityItem } from "@/lib/api";
import { customCodeFailure } from "@/lib/customCode";

/** Plain-language extra for activity kinds whose summary alone is not enough. Raw detail stays in the expandable JSON. */
export function ActivityExtra({ it }: { it: ActivityItem }) {
  if (it.kind !== "custom_code_failed") return null;
  const f = customCodeFailure(it.detail);
  if (!f) return null;
  return (
    <div className="neg" style={{ marginTop: ".3rem" }}>
      Your custom code failed, so this agent did nothing this run. No funds moved.
      <div className="mono" style={{ fontSize: ".8rem" }}>{f.error}</div>
      {f.logs.length > 0 && <details><summary>Logs ({f.logs.length})</summary><pre>{f.logs.join("\n")}</pre></details>}
    </div>
  );
}
