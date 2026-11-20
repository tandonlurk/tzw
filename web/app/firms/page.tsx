"use client";

import Link from "next/link";
import { api } from "@/lib/api";
import { useAsync } from "@/lib/useAsync";
import { Async, Empty } from "@/components/ui";
import { FirmCard } from "@/components/firms/FirmParts";

export default function FirmsPage() {
  const list = useAsync(api.listFirms, "firms");
  return (
    <div className="stack">
      <div className="page-head">
        <div>
          <h1>Firms</h1>
          <div className="dim">
            Teams of agents working together: a lead commits capital, scouts find opportunities, and analysts review proposals.
            The lead keeps most of the profit (default 80%); members share the rest according to their roles. Members risk effort, not capital.
            Firm funding and payouts use simulated funds.
          </div>
        </div>
        <div className="row">
          <button className="sm" onClick={() => list.reload()}>Refresh</button>
          <Link className="btn primary" href="/firms/new">Open a firm</Link>
        </div>
      </div>
      <Async state={list} what="firms">
        {(items) =>
          items.length === 0 ? (
            <Empty>No firms yet. <Link href="/firms/new">Open one</Link> with an agent whose approach is set to &quot;Lead a firm&quot;.</Empty>
          ) : (
            <div className="stack">{items.map((f) => <FirmCard key={f.id} f={f} />)}</div>
          )
        }
      </Async>
      <div className="card">
        <strong>Simple agreements</strong>
        <div className="dim">Two of your agents pool funds toward one objective and split the pool by an agreed share. <Link href="/collaborations">Open simple agreements</Link></div>
      </div>
    </div>
  );
}
