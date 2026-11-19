// Small hand-drawn icon set: 24px grid, 1.5px stroke (matches the 500-weight UI labels). No icon dependency.
import type { SVGProps } from "react";

export type IconName =
  | "overview" | "agents" | "wallet" | "activity" | "settings" | "challenge" | "firm" | "approval"
  | "cocaptain" | "terminal" | "logic" | "code" | "history" | "plus" | "search" | "close" | "chevron-right"
  | "chevron-down" | "play" | "pause" | "check" | "alert" | "external" | "reset" | "grip" | "trash" | "copy"
  | "save" | "basic-mode" | "advanced-mode" | "layout" | "expand" | "fit" | "minimap" | "bolt" | "dot" | "arrow-up-right"
  | "arrow-down-right" | "file" | "branch" | "terminal-window" | "filter" | "refresh";

const P: Record<IconName, React.ReactNode> = {
  overview: <><rect x="3.5" y="3.5" width="7" height="8" rx="1.5" /><rect x="13.5" y="3.5" width="7" height="5" rx="1.5" /><rect x="13.5" y="11.5" width="7" height="9" rx="1.5" /><rect x="3.5" y="14.5" width="7" height="6" rx="1.5" /></>,
  agents: <><rect x="5" y="7.5" width="14" height="11" rx="3" /><path d="M12 4v3.5M9.5 12.5v1M14.5 12.5v1M2.5 12v3M21.5 12v3" /></>,
  wallet: <><path d="M4 7.5A2.5 2.5 0 0 1 6.5 5H17v2.5" /><rect x="4" y="7.5" width="16.5" height="11.5" rx="2.5" /><path d="M16 13.25h1.5" /></>,
  activity: <path d="M3 12h3.5l2.5-6 4.5 12 2.5-6H21" />,
  settings: <><path d="M4 7h9M17 7h3M4 17h3M11 17h9" /><circle cx="15" cy="7" r="2" /><circle cx="9" cy="17" r="2" /></>,
  challenge: <><path d="M5 19 19 5M14 5h5v5" /><path d="m5 5 5.5 5.5M13.5 13.5 19 19M19 14v5h-5" /></>,
  firm: <><rect x="4" y="8" width="16" height="12" rx="2" /><path d="M9 8V5.5A1.5 1.5 0 0 1 10.5 4h3A1.5 1.5 0 0 1 15 5.5V8M4 13h16" /></>,
  approval: <><path d="M12 3.5 5 6v5.5c0 4.2 3 7.6 7 9 4-1.4 7-4.8 7-9V6l-7-2.5Z" /><path d="m9 12 2.2 2.2L15.5 10" /></>,
  cocaptain: <path d="M12 3.5c.6 3.9 2.6 5.9 6.5 6.5-3.9.6-5.9 2.6-6.5 6.5-.6-3.9-2.6-5.9-6.5-6.5 3.9-.6 5.9-2.6 6.5-6.5ZM18.5 15.5c.3 1.7 1.1 2.5 2.5 2.7-1.4.3-2.2 1.1-2.5 2.8-.3-1.7-1.1-2.5-2.5-2.8 1.4-.2 2.2-1 2.5-2.7Z" />,
  terminal: <><path d="M7 4v16M17 4v16M12 7v11" /><rect x="5" y="8" width="4" height="7" rx="1" /><rect x="15" y="6" width="4" height="6" rx="1" /><rect x="10" y="10" width="4" height="5" rx="1" /></>,
  logic: <><circle cx="5.5" cy="12" r="2.5" /><rect x="14.5" y="3.5" width="6" height="5" rx="1.5" /><rect x="14.5" y="15.5" width="6" height="5" rx="1.5" /><path d="M8 12h3m0 0c0-4 1-6 3.5-6M11 12c0 4 1 6 3.5 6" /></>,
  code: <path d="m8.5 7-5 5 5 5M15.5 7l5 5-5 5M13.5 4.5l-3 15" />,
  history: <><path d="M4 12a8 8 0 1 0 2.3-5.6M4 4.5v3.5h3.5" /><path d="M12 8v4.5l3 1.5" /></>,
  plus: <path d="M12 5v14M5 12h14" />,
  search: <><circle cx="11" cy="11" r="6.5" /><path d="m20 20-4.2-4.2" /></>,
  close: <path d="M6 6l12 12M18 6 6 18" />,
  "chevron-right": <path d="m10 6 6 6-6 6" />,
  "chevron-down": <path d="m6 10 6 6 6-6" />,
  play: <path d="M8 5.5v13l10.5-6.5L8 5.5Z" />,
  pause: <path d="M9 5.5v13M15 5.5v13" />,
  check: <path d="m5 12.5 4.5 4.5L19 7.5" />,
  alert: <><path d="M12 4 3 19.5h18L12 4Z" /><path d="M12 10v4M12 16.8v.2" /></>,
  external: <path d="M14 4.5h5.5V10M19.5 4.5 11 13M17 14v4.5a1.5 1.5 0 0 1-1.5 1.5h-10A1.5 1.5 0 0 1 4 18.5v-10A1.5 1.5 0 0 1 5.5 7H10" />,
  reset: <><path d="M4.5 12a7.5 7.5 0 1 0 2.2-5.3M4.5 4.5V9H9" /></>,
  grip: <><circle cx="9" cy="6.5" r=".9" /><circle cx="15" cy="6.5" r=".9" /><circle cx="9" cy="12" r=".9" /><circle cx="15" cy="12" r=".9" /><circle cx="9" cy="17.5" r=".9" /><circle cx="15" cy="17.5" r=".9" /></>,
  trash: <path d="M4.5 7h15M10 4h4M6.5 7l.8 11.6A1.5 1.5 0 0 0 8.8 20h6.4a1.5 1.5 0 0 0 1.5-1.4L17.5 7M10 11v5M14 11v5" />,
  copy: <><rect x="8.5" y="8.5" width="11" height="11" rx="2" /><path d="M15.5 8.5V6A1.5 1.5 0 0 0 14 4.5H6A1.5 1.5 0 0 0 4.5 6v8A1.5 1.5 0 0 0 6 15.5h2.5" /></>,
  save: <><path d="M5 4.5h11l3.5 3.5v10A1.5 1.5 0 0 1 18 19.5H6A1.5 1.5 0 0 1 4.5 18V6" /><path d="M8 4.5V9h7V4.5M8 19.5v-5h8v5" /></>,
  "basic-mode": <><rect x="3.5" y="4.5" width="17" height="15" rx="2.5" /><path d="M9 4.5v15M12.5 9h4.5M12.5 12.5h3" /></>,
  "advanced-mode": <><rect x="3.5" y="4.5" width="17" height="15" rx="2.5" /><path d="m6.5 15.5 3.5-4 3 2.5 4.5-6" /></>,
  layout: <><rect x="3.5" y="3.5" width="17" height="17" rx="2" /><path d="M3.5 10.5h17M12 10.5v10" /></>,
  expand: <path d="M14.5 4.5h5v5M19.5 4.5l-6 6M9.5 19.5h-5v-5M4.5 19.5l6-6" />,
  fit: <path d="M4.5 9V5.5a1 1 0 0 1 1-1H9M15 4.5h3.5a1 1 0 0 1 1 1V9M19.5 15v3.5a1 1 0 0 1-1 1H15M9 19.5H5.5a1 1 0 0 1-1-1V15" />,
  minimap: <><rect x="3.5" y="5.5" width="17" height="13" rx="2" /><rect x="12" y="12" width="6" height="4" rx=".8" /></>,
  bolt: <path d="M13 3.5 5.5 13.5H12l-1 7 7.5-10H12l1-7Z" />,
  dot: <circle cx="12" cy="12" r="3" />,
  "arrow-up-right": <path d="M7 17 17 7M8.5 7H17v8.5" />,
  "arrow-down-right": <path d="M7 7l10 10M17 8.5V17H8.5" />,
  file: <><path d="M13.5 3.5H7A1.5 1.5 0 0 0 5.5 5v14A1.5 1.5 0 0 0 7 20.5h10a1.5 1.5 0 0 0 1.5-1.5V8.5l-5-5Z" /><path d="M13.5 3.5v5h5" /></>,
  branch: <><circle cx="7" cy="5.5" r="2" /><circle cx="7" cy="18.5" r="2" /><circle cx="17" cy="8" r="2" /><path d="M7 7.5v9M17 10c0 4-10 2-10 6.5" /></>,
  "terminal-window": <><rect x="3.5" y="4.5" width="17" height="15" rx="2" /><path d="m7.5 10 2.5 2-2.5 2M12 15h4.5" /></>,
  filter: <path d="M4.5 6h15M7.5 12h9M10.5 18h3" />,
  refresh: <><path d="M19.5 12a7.5 7.5 0 0 1-13.2 4.9M4.5 12a7.5 7.5 0 0 1 13.2-4.9" /><path d="M17.7 3.5v3.6h-3.6M6.3 20.5v-3.6h3.6" /></>,
};

export function Icon({ name, size = 16, ...rest }: { name: IconName; size?: number } & SVGProps<SVGSVGElement>) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
      {...rest}
    >
      {P[name]}
    </svg>
  );
}
