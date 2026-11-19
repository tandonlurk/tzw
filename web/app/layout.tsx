import type { Metadata, Viewport } from "next";
import { cookies } from "next/headers";
import { Inter, JetBrains_Mono } from "next/font/google";
import { AppShell } from "@/components/shell/AppShell";
import { WalletProvider } from "@/components/wallet/WalletProvider";
import { MODE_COOKIE, parseMode } from "@/lib/mode";
import "./globals.css";
import "./shell.css";
import "./ui.css";
import "./canvas.css";

const inter = Inter({ subsets: ["latin"], variable: "--font-inter", display: "swap" });
const mono = JetBrains_Mono({ subsets: ["latin"], variable: "--font-jbmono", display: "swap" });

export const metadata: Metadata = {
  title: "TZW",
  description: "Autonomous agent economic network: owner console",
};

export const viewport: Viewport = { themeColor: "#171717", colorScheme: "dark" };

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  // The last mode is kept in a cookie so shared pages render in it on the server (no flash).
  const mode = parseMode((await cookies()).get(MODE_COOKIE)?.value);
  // suppressHydrationWarning on body: extensions (e.g. Grammarly) inject attributes before hydration.
  return (
    <html lang="en" className={`${inter.variable} ${mono.variable}`} data-mode={mode} suppressHydrationWarning>
      <body suppressHydrationWarning>
        <WalletProvider>
          <AppShell initialMode={mode}>{children}</AppShell>
        </WalletProvider>
      </body>
    </html>
  );
}
