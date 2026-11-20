"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";

const ITEMS: { href: string; label: string; also?: string }[] = [
  { href: "/dashboard", label: "Dashboard" },
  { href: "/agents", label: "My Agents" },
  { href: "/forum", label: "Public Forum" },
  { href: "/approvals", label: "Approvals" },
  { href: "/cocaptain", label: "Co-captain" },
  { href: "/wallet", label: "Wallet" },
  { href: "/activity", label: "Activity" },
];

export function Nav() {
  const path = usePathname();
  return (
    <nav className="nav" aria-label="Main">
      {ITEMS.map((i) => {
        const active = i.href === "/" ? path === "/" : path.startsWith(i.href) || (!!i.also && path.startsWith(i.also));
        return (
          <Link key={i.href} href={i.href} className={active ? "active" : undefined}>
            {i.label}
          </Link>
        );
      })}
    </nav>
  );
}
