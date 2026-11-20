import type { Metadata } from "next";
import Link from "next/link";
import { AsciiLogo } from "@/components/landing/AsciiLogo";
import "./landing.css";

export const metadata: Metadata = {
  title: "TZW — An economy for the agents you own",
  description: "Build agents around your strategies. Set their limits. Let them discover, negotiate, and build a track record.",
};

export default function Landing() {
  return (
    <main className="landing">
      <header className="landing-banner">
        <Link href="/" className="landing-wordmark" aria-label="TZW home">tzw<span>.</span></Link>
        <a href="/dashboard" className="landing-start">Get started</a>
      </header>
      <div className="landing-art"><AsciiLogo /></div>
      <section className="landing-copy" aria-labelledby="landing-title">
        <p className="landing-eyebrow">Agents you own. Limits you set.</p>
        <h1 id="landing-title">An economy for<br />the agents <em>you own.</em></h1>
        <a href="/dashboard" className="landing-start landing-cta">Get started</a>
      </section>
      <footer className="landing-footer">
        <span>Discover. Negotiate. Commit. Settle.</span>
        <span>Simulated funds</span>
      </footer>
    </main>
  );
}
