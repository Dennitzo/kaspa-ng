import type { MetaFunction } from "@react-router/node";
import numeral from "numeral";
import FooterHelper from "../layout/FooterHelper";
import Box from "../assets/box.svg";
import { useMempoolLive } from "../hooks/useMempool";
import Card from "../layout/Card";
import Tooltip, { TooltipDisplayMode } from "../Tooltip";
import { useMemo } from "react";

export const meta: MetaFunction = () => {
  return [
    { title: "Kaspa Mempool | Kaspa Explorer" },
    {
      name: "description",
      content: "Visualize a rolling 1-minute window of Kaspa mempool activity and how transactions stack into the next block.",
    },
    { name: "keywords", content: "Kaspa mempool, fee rate, block template, transactions" },
  ];
};

const formatSompi = (value?: number) => {
  if (!Number.isFinite(value)) return "--";
  return numeral((value || 0) / 1_0000_0000).format("0,0.00[0000]");
};

const formatMass = (value?: number) => {
  if (!Number.isFinite(value)) return "--";
  return numeral(value || 0).format("0,0");
};

const formatFeeRate = (value?: number) => {
  if (!Number.isFinite(value)) return "--";
  return numeral(value || 0).format("0,0.[00]");
};

const bucketLabel = (min: number, max?: number | null) => {
  const range = max === null || max === undefined ? `≥ ${min}` : `${min}–${max}`;
  return `${range} sompi/mass`;
};

const percentile = (values: number[], pct: number) => {
  if (!values.length) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const rank = (pct / 100) * (sorted.length - 1);
  const lower = Math.floor(rank);
  const upper = Math.ceil(rank);
  if (lower === upper) return sorted[lower];
  const weight = rank - lower;
  return sorted[lower] * (1 - weight) + sorted[upper] * weight;
};

const feeRateColor = (value: number, min: number, max: number) => {
  if (!Number.isFinite(value) || max <= min) return "hsl(160 40% 70%)";
  const t = Math.max(0, Math.min(1, (value - min) / (max - min)));
  const hue = 150 - t * 130;
  return `hsl(${hue} 60% 62%)`;
};

const formatAxis = (values: number[]) => {
  if (!values.length) {
    return { maxLabel: "--", unit: "", maxValue: 0 };
  }
  const max = Math.max(...values, 0);
  let unit = "";
  let divisor = 1;
  if (max >= 1_000_000_000) {
    unit = "B";
    divisor = 1_000_000_000;
  } else if (max >= 1_000_000) {
    unit = "M";
    divisor = 1_000_000;
  } else if (max >= 1_000) {
    unit = "k";
    divisor = 1_000;
  }
  return {
    maxLabel: `${numeral(max / divisor).format("0.[0]")}${unit}`,
    unit,
    maxValue: max,
  };
};

const shortenId = (value?: string | null) => {
  if (!value) return "--";
  if (value.length <= 14) return value;
  return `${value.slice(0, 8)}…${value.slice(-6)}`;
};

export default function Mempool() {
  const { summary, history, isConnecting } = useMempoolLive(60);

  const buckets = summary?.buckets || [];
  const tiles = useMemo(() => {
    const seen = new Set<string>();
    const unique: typeof summary.tiles = [];
    for (const tile of summary?.tiles || []) {
      const id = tile?.id;
      if (!id) continue;
      if (seen.has(id)) continue;
      seen.add(id);
      unique.push(tile);
    }
    return unique;
  }, [summary?.tiles]);
  const aggregates = summary?.aggregates || {};
  const blockLimit = summary?.blockMassLimit || 1_000_000;
  const totalMass = summary?.totalMass || 0;
  const feeRateMin = summary?.feeRateMin ?? aggregates?.feeRateMin ?? 0;
  const feeRateMax = summary?.feeRateMax ?? aggregates?.feeRateMax ?? 0;

  const tileSizing = useMemo(() => {
    const masses = tiles.map((tile) => tile.mass || 0).filter((mass) => mass > 0);
    return {
      p50: percentile(masses, 50),
      p80: percentile(masses, 80),
      p95: percentile(masses, 95),
    };
  }, [tiles]);

  const tileScale = useMemo(() => {
    const count = tiles.length;
    if (count <= 120) return 1;
    const over = count - 120;
    const scaled = 1 / (1 + over / 400);
    return Math.max(0.5, scaled);
  }, [tiles.length]);

  const historySeries = useMemo(() => {
    const rows = [...(history || [])].reverse();
    return rows.map((point, index) => ({
      index,
      totalMass: point.totalMass || 0,
      txCount: point.txCount || 0,
      feeRateMedian: point.feeRateMedian || 0,
    }));
  }, [history]);

  const sparkline = (values: number[], height: number = 60) => {
    if (values.length === 0) return "";
    const min = Math.min(0, ...values);
    const max = Math.max(...values);
    const range = Math.max(1, max - min);
    const step = values.length > 1 ? 1000 / (values.length - 1) : 1000;
    return values
      .map((value, index) => {
        const x = index * step;
        const y = height - ((value - min) / range) * height;
        return `${x},${y}`;
      })
      .join(" ");
  };

  return (
    <>
      <section className="relative w-full overflow-hidden rounded-4xl bg-gradient-to-br from-[#f8fbff] via-white to-[#e9fbf7] px-6 py-10 text-black shadow-[0px_20px_60px_-30px_rgba(15,23,42,0.35)] sm:px-10 sm:py-12">
        <div className="pointer-events-none absolute -right-20 -top-20 h-72 w-72 rounded-full bg-[#7dd3fc] opacity-30 blur-3xl" />
        <div className="pointer-events-none absolute -left-24 top-24 h-64 w-64 rounded-full bg-[#34d399] opacity-20 blur-3xl" />
        <div className="relative z-1 mx-auto flex w-full max-w-5xl flex-col items-center text-center">
          <p className="text-xs uppercase tracking-[0.3em] text-gray-400">Kaspa</p>
          <h1 className="max-w-3xl text-3xl font-bold sm:text-4xl uppercase tracking-[0.3em]">Mempool</h1>
          <p className="mt-3 max-w-3xl text-sm text-gray-500">
            This view shows a rolling 1-minute window of mempool activity and approximates how transactions stack into the next block.
          </p>
        </div>
      </section>

      <div className="mt-6 flex w-full flex-col gap-6 rounded-4xl bg-white p-4 text-left text-gray-500 sm:p-8">
        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <Card
            title="Total mempool txs (1 minute)"
            value={numeral(summary?.txCount || 0).format("0,0")}
            variant="analytics"
            loading={isConnecting}
          />
          <Card
            title="Total mass (1 minute)"
            value={formatMass(summary?.totalMass)}
            variant="analytics"
            loading={isConnecting}
          />
          <Card
            title="Total fees (1 minute)"
            value={`${formatSompi(summary?.totalFee)} KAS`}
            variant="analytics"
            loading={isConnecting}
          />
          <Card
            title="Median fee rate (1 minute)"
            value={`${formatFeeRate(summary?.feeRateMedian)} sompi/mass`}
            variant="analytics"
            loading={isConnecting}
          />
        </div>

        <div className="rounded-3xl border border-gray-100 bg-gray-25 p-6">
          <div className="flex flex-col gap-2">
            <span className="text-xs uppercase tracking-[0.3em] text-gray-400">Current mempool block visualisation</span>
            <span className="text-xs text-gray-400">
              Color shows fee rate (higher = warmer). Block size scales with transaction mass.
            </span>
          </div>
          <div className="mt-4 rounded-3xl border border-gray-200 bg-white p-4">
            <div
              className="grid grid-cols-[repeat(24,minmax(0,1fr))] auto-rows-[8px] gap-1"
              style={{ gridAutoFlow: "dense" }}
            >
              {tiles.map((tile, index) => {
                const mass = tile.mass || 0;
                const feeRate = tile.feeRate || 0;
                let colSpan = 2;
                let rowSpan = 2;
                if (mass >= tileSizing.p95) {
                  colSpan = 6;
                  rowSpan = 6;
                } else if (mass >= tileSizing.p80) {
                  colSpan = 5;
                  rowSpan = 5;
                } else if (mass >= tileSizing.p50) {
                  colSpan = 4;
                  rowSpan = 4;
                } else if (mass > 0) {
                  colSpan = 3;
                  rowSpan = 3;
                }
                colSpan = Math.max(1, Math.round(colSpan * tileScale));
                rowSpan = Math.max(1, Math.round(rowSpan * tileScale));
                const statusLabel = tile.confirmed ? "Confirmed (left mempool)" : "In mempool";
                const tooltipMessage = [
                  `TX ${shortenId(tile.id)}`,
                  `Mass: ${formatMass(tile.mass)}`,
                  `Fee: ${formatSompi(tile.fee)} KAS`,
                  `Fee rate: ${formatFeeRate(tile.feeRate)} sompi/mass`,
                  `Status: ${statusLabel}`,
                ].join("\n");
                return (
                  <div
                    key={tile.id || `tile-${index}`}
                    className="h-full w-full"
                    style={{
                      gridColumn: `span ${colSpan} / span ${colSpan}`,
                      gridRow: `span ${rowSpan} / span ${rowSpan}`,
                    }}
                  >
                    <Tooltip
                      message={tooltipMessage}
                      display={TooltipDisplayMode.Hover}
                      multiLine
                      contentClassName="whitespace-pre break-normal max-w-none overflow-visible"
                      className="block h-full w-full"
                      triggerClassName="block h-full w-full"
                    >
                      <div
                        title={`mass ${numeral(mass).format("0,0")} | fee rate ${formatFeeRate(feeRate)}`}
                        className="h-full w-full rounded-md"
                        style={{
                          background: feeRateColor(feeRate, feeRateMin, feeRateMax),
                        }}
                      />
                    </Tooltip>
                  </div>
                );
              })}
            </div>
            {tiles.length === 0 && <div className="text-sm text-gray-500">No mempool data yet.</div>}
          </div>
          <div className="mt-3 text-xs text-gray-500">
            Total mempool mass: {numeral(totalMass).format("0,0")} of {numeral(blockLimit * 60).format("0,0")} (1 minute)
          </div>
        </div>

        <div className="grid grid-cols-1 gap-6 lg:grid-cols-2">
          <div className="rounded-3xl border border-gray-100 bg-white p-6">
            <div className="text-sm uppercase tracking-[0.3em] text-gray-400">Mempool mass (1 minute)</div>
            <div className="mt-4 flex items-stretch gap-3">
              <div className="flex w-10 flex-col justify-between text-[10px] text-gray-400">
                <span>{formatAxis(historySeries.map((row) => row.totalMass)).maxLabel}</span>
                <span>0</span>
              </div>
              <svg className="h-24 w-full" viewBox="0 0 1000 60" preserveAspectRatio="none">
                <line x1="0" y1="0" x2="0" y2="60" stroke="#E5E7EB" strokeWidth="2" />
                <polyline
                  fill="none"
                  stroke="#70C7BA"
                  strokeWidth="3"
                  points={sparkline(historySeries.map((row) => row.totalMass))}
                />
              </svg>
            </div>
            <div className="mt-2 text-xs text-gray-500">
              Latest {numeral(summary?.totalMass || 0).format("0,0")} mass (1 minute)
            </div>
          </div>
          <div className="rounded-3xl border border-gray-100 bg-white p-6">
            <div className="text-sm uppercase tracking-[0.3em] text-gray-400">Mempool transactions (1 minute)</div>
            <div className="mt-4 flex items-stretch gap-3">
              <div className="flex w-10 flex-col justify-between text-[10px] text-gray-400">
                <span>{formatAxis(historySeries.map((row) => row.txCount)).maxLabel}</span>
                <span>0</span>
              </div>
              <svg className="h-24 w-full" viewBox="0 0 1000 60" preserveAspectRatio="none">
                <line x1="0" y1="0" x2="0" y2="60" stroke="#E5E7EB" strokeWidth="2" />
                <polyline
                  fill="none"
                  stroke="#49EACB"
                  strokeWidth="3"
                  points={sparkline(historySeries.map((row) => row.txCount))}
                />
              </svg>
            </div>
            <div className="mt-2 text-xs text-gray-500">
              Latest {numeral(summary?.txCount || 0).format("0,0")} txs (1 minute)
            </div>
          </div>
        </div>

        <div className="rounded-3xl border border-gray-100 bg-white p-6">
          <div className="text-sm uppercase tracking-[0.3em] text-gray-400">Fee-rate buckets (1 minute)</div>
          <div className="mt-1 text-xs text-gray-400">
            Aggregated transaction mass grouped by fee-rate ranges over the last minute.
          </div>
          <div className="mt-4 grid grid-cols-1 gap-2 sm:grid-cols-2 lg:grid-cols-3">
            {buckets.map((bucket, index) => (
              <div key={`${bucket.min}-${bucket.max}`} className="flex items-center justify-between rounded-2xl bg-gray-25 px-4 py-3">
                <span className="text-sm text-gray-600">{bucketLabel(bucket.min, bucket.max)}</span>
                <span className="text-sm text-black">{numeral(bucket.mass || 0).format("0,0")} mass</span>
              </div>
            ))}
            {buckets.length === 0 && <div className="text-sm text-gray-500">No mempool data yet.</div>}
          </div>
        </div>
      </div>
      <FooterHelper icon={Box}>
        <span>
          This view shows a rolling 1-minute window of mempool activity and approximates how transactions stack into the next block.
        </span>
      </FooterHelper>
    </>
  );
}
