import type { Metadata } from "next";
import { Nav } from "@/components/Nav";
import { WalletProvider } from "@/components/wallet/WalletProvider";
import "./globals.css";

export const metadata: Metadata = {
  title: "Agent Economic Network",
  description: "Owner console for autonomous, user-owned agents",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  // suppressHydrationWarning on body: extensions (e.g. Grammarly) inject attributes before hydration.
  return (
    <html lang="en">
      <body suppressHydrationWarning>
        <div className="shell">
          <aside className="sidebar">
            <div className="brand">Agent Network</div>
            <Nav />
          </aside>
          <main className="main"><WalletProvider>{children}</WalletProvider></main>
        </div>
      </body>
    </html>
  );
}
