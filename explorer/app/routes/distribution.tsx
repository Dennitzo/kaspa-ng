import type { MetaFunction } from "@react-router/node";
import { useMemo } from "react";
import numeral from "numeral";
import FooterHelper from "../layout/FooterHelper";
import { VPROGS_BASE } from "../api/urls";
import PageTable from "../PageTable";
import AccountBalanceWallet from "../assets/account_balance_wallet.svg";
import { useAddressDistribution } from "../hooks/useAddressDistribution";

export const meta: MetaFunction = () => {
  return [
    { title: "Kaspa Distribution | Kaspa Explorer" },
    {
      name: "description",
      content:
        "Explore Kaspa balance distribution tiers. See how many addresses fall into each balance bucket and their share of supply.",
    },
    {
      name: "keywords",
      content: "Kaspa distribution, address tiers, whale chart, balance buckets",
    },
  ];
};

const KAS_SOMPI = 1_0000_0000;

const TIER_BADGES: Record<string, string> = {
  aquaman: "🧜‍♂️",
  humpback: "🐋",
  whale: "🐳",
  shark: "🦈",
  dolphin: "🐬",
  fish: "🐟",
  octopus: "🐙",
  crab: "🦀",
  shrimp: "🦐",
  oyster: "🦪",
  plankton: "🦠",
};

const formatKasCompact = (value: number) => {
  if (!Number.isFinite(value)) return "--";
  if (value >= 1_000_000_000) return `${numeral(value / 1_000_000_000).format("0.[00]")} Mrd.`;
  if (value >= 1_000_000) return `${numeral(value / 1_000_000).format("0.[00]")} Mio.`;
  if (value >= 1_000) return numeral(value).format("0,0");
  if (value >= 1) return numeral(value).format("0,0.[00]");
  return numeral(value).format("0.[0000]");
};

const formatKasTotal = (sompi: number) => {
  if (!Number.isFinite(sompi)) return "--";
  const kas = sompi / KAS_SOMPI;
  return `${numeral(kas).format("0,0.00[0000]")} KAS`;
};

export default function Distribution() {
  const { data, isLoading, isError } = useAddressDistribution(60000);
  const vprogsBaseUrl = VPROGS_BASE;

  const tiers = useMemo(() => {
    const rows = data?.tiers || [];
    return [...rows].sort((a, b) => (b.minKas || 0) - (a.minKas || 0));
  }, [data]);

  const rows = [...tiers].reverse().map((tier) => [
    <span
      key={`${tier.id}-badge`}
      className="inline-flex h-9 w-9 items-center justify-center rounded-full bg-gray-100 text-base font-semibold text-gray-600"
    >
      {TIER_BADGES[tier.id] || "??"}
    </span>,
    <span key={`${tier.id}-name`} className="font-medium text-gray-700">{tier.name}</span>,
    <span key={`${tier.id}-min`} className="text-gray-700">{formatKasCompact(tier.minKas)} KAS</span>,
    <span key={`${tier.id}-count`} className="text-gray-700">{numeral(tier.count || 0).format("0,0")}</span>,
    <span key={`${tier.id}-share`} className="text-gray-700">
      {tier.sharePct != null ? `${numeral(tier.sharePct).format("0.00")}%` : "--"}
    </span>,
    <span key={`${tier.id}-total`} className="text-gray-700">{formatKasTotal(tier.totalSompi || 0)}</span>,
  ]);

  return (
    <>
      <section className="relative w-full overflow-hidden rounded-4xl bg-gradient-to-br from-[#f8fbff] via-white to-[#e9fbf7] px-6 py-10 text-black shadow-[0px_20px_60px_-30px_rgba(15,23,42,0.35)] sm:px-10 sm:py-12">
        <div className="pointer-events-none absolute -right-20 -top-20 h-72 w-72 rounded-full bg-[#7dd3fc] opacity-30 blur-3xl" />
        <div className="pointer-events-none absolute -left-24 top-24 h-64 w-64 rounded-full bg-[#34d399] opacity-20 blur-3xl" />
        <div className="relative z-1 mx-auto flex w-full max-w-5xl flex-col items-center text-center">
          <p className="text-xs uppercase tracking-[0.3em] text-gray-400">Kaspa</p>
          <h1 className="max-w-3xl text-3xl font-bold sm:text-4xl uppercase tracking-[0.3em]">Distribution</h1>
          <p className="mt-3 max-w-3xl text-sm text-gray-500">
            Distribution tiers group addresses by balance. Larger tiers represent fewer addresses holding significant
            portions of the circulating supply.
          </p>
        </div>
      </section>

      <div className="mt-6 flex w-full flex-col rounded-4xl bg-white p-4 text-left text-gray-500 sm:p-8">
        <div className="flex items-center justify-end text-xs uppercase tracking-[0.3em] text-gray-400">
          Data computed by vProgs
        </div>
        {isLoading ? (
          <div className="py-12 text-center text-sm text-gray-500">Loading distribution...</div>
        ) : isError || rows.length === 0 ? (
          <div className="py-12 text-center text-sm text-gray-500">Run vProgs job first.</div>
        ) : (
          <>
            <PageTable
              className="text-black"
              headers={["Tier", "Name", "Min amount", "Count", "Share", "Total"]}
              rows={rows}
              rowClassName={(index) => (index % 2 === 1 ? "bg-gray-25" : "")}
              additionalClassNames={{
                0: "w-10 text-left",
                1: "text-left",
                2: "text-left",
                3: "text-left",
                4: "text-left",
                5: "text-left",
              }}
            />
          </>
        )}
      </div>
      <FooterHelper icon={AccountBalanceWallet}>
        <span>
          Distribution tiers group addresses by balance. Larger tiers represent fewer addresses holding significant
          portions of the circulating supply.
        </span>
      </FooterHelper>
    </>
  );
}
