// Small line icons for the lifecycle stations. Stroke uses currentColor.
import React from "react";

const P: React.FC<{ size: number; children: React.ReactNode }> = ({ size, children }) => (
  <svg width={size} height={size} viewBox="0 0 24 24" style={{ fill: "none", stroke: "currentColor", strokeWidth: 1.6, strokeLinecap: "round", strokeLinejoin: "round" }}>{children}</svg>
);

export const ICONS: Record<string, React.FC<{ size?: number }>> = {
  Discover: ({ size = 40 }) => <P size={size}><circle cx="10.5" cy="10.5" r="6" /><path d="M15 15l5 5" /><path d="M7.5 11.5l2-2 2 2 2.5-3" /></P>,
  Negotiate: ({ size = 40 }) => <P size={size}><path d="M4 8h13l-3-3" /><path d="M20 16H7l3 3" /></P>,
  "Check limits": ({ size = 40 }) => <P size={size}><path d="M12 3l7 3v6c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6z" /><path d="M9 12l2 2 4-4" /></P>,
  Lock: ({ size = 40 }) => <P size={size}><rect x="5" y="10.5" width="14" height="10" rx="2" /><path d="M8.5 10.5V7.5a3.5 3.5 0 0 1 7 0v3" /></P>,
  Resolve: ({ size = 40 }) => <P size={size}><path d="M12 4v16" /><path d="M5 20h14" /><path d="M5 8h14" /><path d="M5 8l-2.5 5h5z" /><path d="M19 8l-2.5 5h5z" /></P>,
  Record: ({ size = 40 }) => <P size={size}><rect x="5" y="3.5" width="14" height="17" rx="2" /><path d="M8.5 8h7M8.5 12h7M8.5 16h4" /></P>,
};
