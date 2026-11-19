// Stable identity colours for agents (chart lines, avatars). Same agent set -> same colours on every page.
// All hues share one OKLCH lightness and chroma so no agent looks more important than another.
// Values reference CSS custom properties defined in globals.css (--agent-0 .. --agent-7).

export const AGENT_COLOR_COUNT = 8;
const DASHES = ["", "6 4", "2 3"];

/** Index of an agent among all known agent ids (sorted, so order of fetch does not matter). */
export function agentIndex(id: string, allIds: string[]): number {
  const sorted = [...new Set(allIds)].sort();
  const i = sorted.indexOf(id);
  if (i >= 0) return i;
  let h = 0;
  for (const c of id) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return h;
}

export function agentColor(index: number): string {
  return `var(--agent-${index % AGENT_COLOR_COUNT})`;
}

/** Line dash pattern; past 8 agents, lines repeat colours with a different dash so they stay distinct. */
export function agentDash(index: number): string {
  return DASHES[Math.floor(index / AGENT_COLOR_COUNT) % DASHES.length];
}

/** Two-letter monogram, ignoring a trailing "(simulated)" style suffix. */
export function initials(name: string): string {
  const words = name.replace(/\(.*?\)/g, "").trim().split(/\s+/).filter(Boolean);
  if (words.length === 0) return "?";
  if (words.length === 1) return words[0].slice(0, 2).toUpperCase();
  return (words[0][0] + words[1][0]).toUpperCase();
}

/** Display name without the "(simulated)" suffix (the environment is shown as a badge instead). */
export function shortName(name: string): string {
  return name.replace(/\s*\((simulated|arc[_ ]testnet)\)\s*$/i, "").trim() || name;
}

export interface AgentStyle { index: number; color: string; dash: string }

/** Colour + dash for every agent id in one call (stable across pages for the same agent set). */
export function agentStyles(ids: string[]): Map<string, AgentStyle> {
  const sorted = [...new Set(ids)].sort();
  return new Map(sorted.map((id, index) => [id, { index, color: agentColor(index), dash: agentDash(index) }]));
}
