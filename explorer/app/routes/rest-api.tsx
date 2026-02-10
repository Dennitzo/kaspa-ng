import { API_BASE } from "../api/urls";
import Box from "../assets/box.svg";
import FooterHelper from "../layout/FooterHelper";
import type { Route } from "./+types/rest-api";

export function meta(): Route.MetaFunction {
  return [
    { title: "Kaspa REST API | Explorer" },
    {
      name: "description",
      content:
        "Explore every kaspa-rest-server endpoint grouped by purpose, with concise descriptions and a friendly visual guide.",
    },
  ];
}

type ApiEndpoint = {
  method: "GET" | "POST";
  path: string;
  summary: string;
  note?: string;
  badge?: "deprecated";
};

type ApiSection = {
  id: string;
  title: string;
  description: string;
  endpoints: ApiEndpoint[];
};

const API_SECTIONS: ApiSection[] = [
  {
    id: "network",
    title: "Network & Protocol",
    description: "Live state of the Kaspa network, node health, and supply metrics.",
    endpoints: [
      { method: "GET", path: "/info/health", summary: "Service + indexer health status." },
      { method: "GET", path: "/info/kaspad", summary: "Connected node info (version, sync, mempool size)." },
      { method: "GET", path: "/info/blockdag", summary: "BlockDAG stats (tips, difficulty, DAA, etc.)." },
      { method: "GET", path: "/info/network", summary: "Legacy BlockDAG endpoint.", badge: "deprecated" },
      { method: "GET", path: "/info/virtual-chain-blue-score", summary: "Current virtual chain blue score." },
      { method: "GET", path: "/virtual-chain", summary: "Virtual chain data (current chain structure)." },
      { method: "GET", path: "/info/hashrate", summary: "Estimated network hashrate." },
      { method: "GET", path: "/info/hashrate/max", summary: "Maximum observed network hashrate." },
      { method: "GET", path: "/info/hashrate/history", summary: "Hashrate history (default window)." },
      { method: "GET", path: "/info/hashrate/history/{day_or_month}", summary: "Hashrate history by day/month." },
      { method: "GET", path: "/info/fee-estimate", summary: "Fee estimate buckets for transactions." },
      { method: "GET", path: "/info/halving", summary: "Halving countdown and next halving data." },
      { method: "GET", path: "/info/blockreward", summary: "Current block reward." },
      { method: "GET", path: "/info/coinsupply", summary: "Total + circulating supply snapshot." },
      { method: "GET", path: "/info/coinsupply/circulating", summary: "Circulating supply only." },
      { method: "GET", path: "/info/coinsupply/total", summary: "Total supply only." },
      { method: "GET", path: "/info/marketcap", summary: "Market cap (price * supply)." },
      { method: "GET", path: "/info/price", summary: "Price snapshot." },
      { method: "GET", path: "/info/market-data", summary: "Extended market data payload." },
      { method: "GET", path: "/info/get-vscp-from-block", summary: "VSPC lookup by block." },
    ],
  },
  {
    id: "blocks",
    title: "Blocks & Miners",
    description: "Block listings, lookup by hash/blue score, and miner summaries.",
    endpoints: [
      { method: "GET", path: "/blocks", summary: "Paginated block list." },
      { method: "GET", path: "/blocks/{blockId}", summary: "Block details by hash." },
      { method: "GET", path: "/blocks-from-bluescore", summary: "Blocks near a given blue score." },
      { method: "GET", path: "/blocks/search/miner-info", summary: "Search blocks by miner metadata." },
      { method: "GET", path: "/miners/summary", summary: "Miner stats summary for recent blocks." },
    ],
  },
  {
    id: "transactions",
    title: "Transactions",
    description: "Search, inspect, and submit transactions.",
    endpoints: [{ method: "GET", path: "/transactions/{transactionId}", summary: "Transaction details by ID." }],
  },
  {
    id: "addresses",
    title: "Addresses",
    description: "Balances, flows, activity, and address-level summaries.",
    endpoints: [
      { method: "GET", path: "/addresses/{address}/balance", summary: "Balance for a single address." },
      { method: "GET", path: "/addresses/{address}/balance-flow", summary: "Balance flow timeline for an address." },
      { method: "GET", path: "/addresses/{address}/balance-flow/latest", summary: "Latest balance flow points only." },
      { method: "GET", path: "/addresses/{kaspaAddress}/transactions-count", summary: "Total transaction count for an address." },
      { method: "GET", path: "/addresses/{kaspaAddress}/full-transactions", summary: "Full transaction list for an address." },
      { method: "GET", path: "/addresses/{kaspaAddress}/full-transactions-page", summary: "Paginated full transactions." },
      { method: "GET", path: "/addresses/names", summary: "Address names registry (if configured)." },
      { method: "GET", path: "/addresses/{kaspaAddress}/name", summary: "Name lookup for a single address." },
      { method: "GET", path: "/addresses/top", summary: "Top addresses by balance." },
      { method: "GET", path: "/addresses/distribution", summary: "Balance distribution buckets." },
      { method: "GET", path: "/addresses/active/count/", summary: "Active address count (default window)." },
      { method: "GET", path: "/addresses/active/count/{day_or_month}", summary: "Active address count by day/month." },
    ],
  },
  {
    id: "utxos",
    title: "UTXO Tools",
    description: "Inspect unspent outputs for addresses.",
    endpoints: [
      { method: "GET", path: "/addresses/{kaspaAddress}/utxos", summary: "UTXOs for a single address." },
    ],
  },
];

const methodStyles: Record<ApiEndpoint["method"], { text: string; bg: string; ring: string }> = {
  GET: { text: "text-[#70C7BA]", bg: "bg-transparent", ring: "ring-transparent" },
  POST: { text: "text-sky-700", bg: "bg-sky-100", ring: "ring-sky-200" },
};

const badgeStyles: Record<NonNullable<ApiEndpoint["badge"]>, string> = {
  deprecated: "bg-amber-100 text-amber-700 ring-amber-200",
};

const EXAMPLE_ADDRESS = "kaspa:ppk66xua7nmq8elv3eglfet0xxcfuks835xdgsm5jlymjhazyu6h5ac62l4ey";
const EXAMPLE_TX_ID = "93bed3819444266f630d96938fb6d21faead000b5ed27919e7a87e702848239d";
const EXAMPLE_BLOCK_ID = "759662c726c85f3e881a6532f3add15ae512a4edde564baf6df805e39a0a3fe5";
const EXAMPLE_DAY_OR_MONTH = "2026-02-02";

const withExampleValues = (path: string) =>
  path
    .replace("{address}", EXAMPLE_ADDRESS)
    .replace("{kaspaAddress}", EXAMPLE_ADDRESS)
    .replace("{transactionId}", EXAMPLE_TX_ID)
    .replace("{blockId}", EXAMPLE_BLOCK_ID)
    .replace("{day_or_month}", EXAMPLE_DAY_OR_MONTH);

export default function RestApi() {
  return (
    <div className="flex w-full flex-col gap-8">
      <section className="relative w-full overflow-hidden rounded-4xl bg-gradient-to-br from-[#f8fbff] via-white to-[#e9fbf7] px-6 py-10 text-black shadow-[0px_20px_60px_-30px_rgba(15,23,42,0.35)] sm:px-10 sm:py-12">
        <div className="pointer-events-none absolute -right-20 -top-20 h-72 w-72 rounded-full bg-[#7dd3fc] opacity-30 blur-3xl" />
        <div className="pointer-events-none absolute -left-24 top-24 h-64 w-64 rounded-full bg-[#34d399] opacity-20 blur-3xl" />
        <div className="relative z-1 mx-auto flex w-full max-w-5xl flex-col items-center text-center">
          <p className="text-xs uppercase tracking-[0.3em] text-gray-400">Kaspa</p>
          <h1 className="mt-3 text-3xl font-bold sm:text-4xl uppercase tracking-[0.3em]">Rest API</h1>
          <p className="mt-3 max-w-3xl text-sm text-gray-500">
            Reference endpoints for explorer data and network analytics.
          </p>
          <div className="mt-6 flex flex-col items-center gap-3 sm:flex-row sm:items-center sm:justify-center">
            <div className="flex items-center gap-2 rounded-full bg-white/80 px-4 py-2 text-xs text-gray-600 shadow-sm ring-1 ring-gray-100">
              <span className="text-gray-400">Base URL</span>
              <span className="rounded-full bg-gray-100 px-2 py-1 font-mono text-gray-700">{API_BASE}</span>
            </div>
            <div className="flex items-center gap-2 text-xs text-gray-500">
              <span className="h-2 w-2 rounded-full" style={{ backgroundColor: "#70C7BA" }} />
              GET
            </div>
          </div>
          <div className="mt-6 flex flex-wrap justify-center gap-2">
            {API_SECTIONS.map((section) => (
              <a
                key={section.id}
                href={`#${section.id}`}
                className="rounded-full border px-4 py-2 text-xs font-medium transition hover:bg-gray-50"
                style={{ borderColor: "#70C7BA", backgroundColor: "transparent", color: "#70C7BA" }}
              >
                {section.title}
              </a>
            ))}
          </div>
        </div>
      </section>

      {API_SECTIONS.map((section) => (
        <section key={section.id} id={section.id} className="mt-6 rounded-4xl bg-white p-6 text-black shadow-sm sm:p-8">
          <div className="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
            <div>
              <h2 className="text-xl font-semibold">{section.title}</h2>
              <p className="mt-1 text-sm text-gray-500">{section.description}</p>
            </div>
            <div className="text-xs text-gray-400">{section.endpoints.length} endpoints</div>
          </div>

          <div className="mt-6 grid gap-4 lg:grid-cols-2">
            {section.endpoints.map((endpoint) => {
              const methodStyle = methodStyles[endpoint.method];
              const endpointUrl = `${API_BASE.replace(/\/$/, "")}${withExampleValues(endpoint.path)}`;
              return (
                <a
                  key={`${endpoint.method}-${endpoint.path}`}
                  href={endpointUrl}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="flex h-full flex-col gap-3 rounded-3xl border border-gray-100 bg-gradient-to-br from-white to-gray-25 p-5 shadow-sm transition hover:-translate-y-0.5 hover:border-emerald-200 hover:shadow-md"
                >
                  <div className="flex items-center gap-3">
                    <span
                      className={`rounded-full border px-3 py-1 text-xs font-semibold ${methodStyle.text}`}
                      style={{ borderColor: "#70C7BA", backgroundColor: "transparent" }}
                    >
                      {endpoint.method}
                    </span>
                    <span className="font-mono text-xs text-gray-700">{endpoint.path}</span>
                    {endpoint.badge && (
                      <span
                        className={`rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide ring-1 ${badgeStyles[endpoint.badge]}`}
                      >
                        {endpoint.badge}
                      </span>
                    )}
                  </div>
                  <p className="text-sm text-gray-600">{endpoint.summary}</p>
                  {endpoint.note && <p className="text-xs text-gray-400">{endpoint.note}</p>}
                  <div className="text-[11px] font-medium uppercase tracking-[0.2em]" style={{ color: "#70C7BA" }}>
                    Open endpoint
                  </div>
                </a>
              );
            })}
          </div>
        </section>
      ))}
      <FooterHelper icon={Box}>
        <span>Reference endpoints for explorer data and network analytics.</span>
      </FooterHelper>
    </div>
  );
}
