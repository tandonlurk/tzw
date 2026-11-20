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
  Market: ({ size = 40 }) => <P size={size}><path d="M4 19h16" /><path d="M6 15l4-5 3 3 5-7" /><path d="M15 6h3v3" /></P>,
  Model: ({ size = 40 }) => <P size={size}><path d="M12 3l1.8 4.7L18.5 9.5l-4.7 1.8L12 16l-1.8-4.7L5.5 9.5l4.7-1.8z" /><path d="M18 15l.8 2.2L21 18l-2.2.8L18 21l-.8-2.2L15 18l2.2-.8z" /></P>,
  Server: ({ size = 40 }) => <P size={size}><rect x="4" y="4" width="16" height="7" rx="1.5" /><rect x="4" y="13" width="16" height="7" rx="1.5" /><path d="M8 7.5h.01M8 16.5h.01" /></P>,
  Vault: ({ size = 40 }) => <P size={size}><rect x="3.5" y="4.5" width="17" height="15" rx="2" /><circle cx="12" cy="12" r="3.5" /><path d="M12 8.5v1M12 14.5v1M8.5 12h1M14.5 12h1" /></P>,
  Person: ({ size = 40 }) => <P size={size}><circle cx="12" cy="8" r="3.5" /><path d="M5 20c1-3.5 3.8-5.5 7-5.5s6 2 7 5.5" /></P>,
  Check: ({ size = 40 }) => <P size={size}><path d="M5 12.5l4.5 4.5L19 7.5" /></P>,
  Clock: ({ size = 40 }) => <P size={size}><circle cx="12" cy="12" r="8" /><path d="M12 7.5V12l3 2" /></P>,
  Record: ({ size = 40 }) => <P size={size}><rect x="5" y="3.5" width="14" height="17" rx="2" /><path d="M8.5 8h7M8.5 12h7M8.5 16h4" /></P>,
};
