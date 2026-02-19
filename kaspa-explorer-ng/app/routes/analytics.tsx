import KasLink from "../KasLink";
import PageTable from "../PageTable";
import ArrowRight from "../assets/arrow-right.svg";
import AnalyticsIcon from "../assets/analytics.svg";
import BarChart from "../assets/bar_chart.svg";
import Box from "../assets/box.svg";
import Coins from "../assets/coins.svg";
import FlashOn from "../assets/flash_on.svg";
import PieChart from "../assets/pie_chart.svg";
import Reward from "../assets/reward.svg";
import Swap from "../assets/swap.svg";
import FooterHelper from "../layout/FooterHelper";
import { useBlockdagInfo } from "../hooks/useBlockDagInfo";
import { useBlockReward } from "../hooks/useBlockReward";
import { useCoinSupply } from "../hooks/useCoinSupply";
import { useFeeEstimate } from "../hooks/useFeeEstimate";
import { useHalving } from "../hooks/useHalving";
import { useHashrate } from "../hooks/useHashrate";
import { useIncomingBlocks } from "../hooks/useIncomingBlocks";
import { useMempoolSize } from "../hooks/useMempoolSize";
import dayjs from "dayjs";
import numeral from "numeral";
import React, { useEffect, useMemo, useRef, useState } from "react";

export function meta() {
  return [
    { title: "Kaspa Analytics - Network Stats & Charts | Kaspa Explorer" },
    {
      name: "description",
      content:
        "Analyze the Kaspa blockchain with real-time charts and statistics. Track block production, hash rate, difficulty, and network growth.",
    },
    {
      name: "keywords",
      content: "Kaspa analytics, blockchain stats, network charts, hash rate, difficulty, block time",
    },
  ];
}

const TOTAL_SUPPLY = 28_700_000_000;

const formatDifficulty = (value: number) => {
  if (!Number.isFinite(value) || value <= 0) {
    return { value: "0", unit: "" };
  }

  const units = ["", "K", "M", "G", "T", "P", "E"];
  let unitIndex = 0;
  let scaled = value;
  while (scaled >= 1000 && unitIndex < units.length - 1) {
    scaled /= 1000;
    unitIndex += 1;
  }

  return {
    value: numeral(scaled).format("0,0.[00]"),
    unit: units[unitIndex],
  };
};

const formatHashrate = (valueTh: number) => {
  if (!Number.isFinite(valueTh) || valueTh <= 0) {
    return { value: "0", unit: "H/s" };
  }

  // API returns TH/s; convert to H/s to allow downscaling to MH/s, GH/s, etc.
  const units = ["H/s", "KH/s", "MH/s", "GH/s", "TH/s", "PH/s", "EH/s", "ZH/s"];
  let unitIndex = 0;
  let scaled = valueTh * 1e12;
  while (scaled >= 1000 && unitIndex < units.length - 1) {
    scaled /= 1000;
    unitIndex += 1;
  }
  return { value: numeral(scaled).format("0,0.[00]"), unit: units[unitIndex] };
};

export default function Analytics() {
  const { data: blockDagInfo, isLoading: isLoadingBlockDagInfo } = useBlockdagInfo();
  const { data: coinSupply, isLoading: isLoadingCoinSupply } = useCoinSupply();
  const { data: blockReward, isLoading: isLoadingBlockReward } = useBlockReward();
  const { data: halving, isLoading: isLoadingHalving } = useHalving();
  const { data: hashrate, isLoading: isLoadingHashrate } = useHashrate();
  const { data: feeEstimate, isLoading: isLoadingFee } = useFeeEstimate();
  const { mempoolSize } = useMempoolSize();
  const { blocks, avgBlockTime, avgTxRate, transactions } = useIncomingBlocks();
  const [hideCoinbaseOnlyBlocks, setHideCoinbaseOnlyBlocks] = useState(true);
  const [hideCoinbaseTxs, setHideCoinbaseTxs] = useState(true);
  const [pauseBlocks, setPauseBlocks] = useState(false);
  const [pauseTransactions, setPauseTransactions] = useState(false);
  const frozenBlocksRef = useRef<typeof blocks>([]);
  const frozenTransactionsRef = useRef<typeof transactions>([]);
  const [hashrateRange, setHashrateRange] = useState<{ min: number; max: number }>({ min: 0, max: 0 });
  const [difficultyRange, setDifficultyRange] = useState<{ min: number; max: number }>({ min: 0, max: 0 });

  const hashrateDisplay = isLoadingHashrate ? { value: "--", unit: "" } : formatHashrate(hashrate?.hashrate ?? 0);
  const difficultyDisplay = isLoadingBlockDagInfo
    ? { value: "--", unit: "" }
    : formatDifficulty(blockDagInfo?.difficulty ?? 0);

  const circulatingSupply = (coinSupply?.circulatingSupply || 0) / 1_0000_0000;
  const minedPercent = (circulatingSupply / TOTAL_SUPPLY) * 100;
  const baseFeeRate = Number(feeEstimate?.normalBuckets?.[0]?.feerate ?? NaN);
  const regularFee = Number.isFinite(baseFeeRate) ? (baseFeeRate * 2036) / 1_0000_0000 : NaN;
  const mempoolSizeValue = Number(mempoolSize) || 0;
  const [mempoolRange, setMempoolRange] = useState<{ min: number; max: number }>({ min: 0, max: 0 });
  const mempoolPercent =
    mempoolRange.max > mempoolRange.min
      ? ((mempoolSizeValue - mempoolRange.min) / (mempoolRange.max - mempoolRange.min)) * 100
      : mempoolRange.max > 0
        ? 100
        : 0;

  useEffect(() => {
    const current = Number(hashrate?.hashrate ?? 0);
    if (!Number.isFinite(current) || current <= 0) return;
    setHashrateRange((prev) => ({
      min: prev.min === 0 ? current : Math.min(prev.min, current),
      max: Math.max(prev.max, current),
    }));
  }, [hashrate]);

  useEffect(() => {
    const current = Number(blockDagInfo?.difficulty ?? 0);
    if (!Number.isFinite(current) || current <= 0) return;
    setDifficultyRange((prev) => ({
      min: prev.min === 0 ? current : Math.min(prev.min, current),
      max: Math.max(prev.max, current),
    }));
  }, [blockDagInfo]);

  useEffect(() => {
    const current = mempoolSizeValue;
    if (!Number.isFinite(current) || current <= 0) return;
    setMempoolRange((prev) => ({
      min: prev.min === 0 ? current : Math.min(prev.min, current),
      max: Math.max(prev.max, current),
    }));
  }, [mempoolSizeValue]);

  const hashratePercent =
    hashrateRange.max > hashrateRange.min
      ? ((Number(hashrate?.hashrate ?? 0) - hashrateRange.min) / (hashrateRange.max - hashrateRange.min)) * 100
      : hashrateRange.max > 0
        ? 100
        : 0;

  const difficultyPercent =
    difficultyRange.max > difficultyRange.min
      ? ((Number(blockDagInfo?.difficulty ?? 0) - difficultyRange.min) / (difficultyRange.max - difficultyRange.min)) *
        100
      : difficultyRange.max > 0
        ? 100
        : 0;

  const filteredBlocks = useMemo(() => {
    if (!hideCoinbaseOnlyBlocks) return blocks;
    return blocks.filter((block) => {
      const txCount = typeof block.txCount === "number" ? block.txCount : block.txs?.length ?? 0;
      return txCount > 1;
    });
  }, [blocks, hideCoinbaseOnlyBlocks]);

  const coinbaseTxIds = useMemo(() => {
    const ids = new Set<string>();
    for (const block of blocks) {
      const firstTx = block.txs?.[0];
      if (firstTx?.txId) ids.add(firstTx.txId);
    }
    return ids;
  }, [blocks]);

  const filteredTransactions = useMemo(() => {
    if (!hideCoinbaseTxs) return transactions;
    return transactions.filter((tx) => !coinbaseTxIds.has(tx.txId));
  }, [transactions, hideCoinbaseTxs, coinbaseTxIds]);

  const displayedBlocks = useMemo(() => {
    if (!pauseBlocks) return filteredBlocks;
    return frozenBlocksRef.current;
  }, [filteredBlocks, pauseBlocks]);

  const displayedTransactions = useMemo(() => {
    if (!pauseTransactions) return filteredTransactions;
    return frozenTransactionsRef.current;
  }, [filteredTransactions, pauseTransactions]);

  useEffect(() => {
    if (!pauseBlocks) {
      frozenBlocksRef.current = filteredBlocks;
    }
  }, [filteredBlocks, pauseBlocks]);

  useEffect(() => {
    if (!pauseTransactions) {
      frozenTransactionsRef.current = filteredTransactions;
    }
  }, [filteredTransactions, pauseTransactions]);

  return (
    <div className="flex w-full flex-col gap-y-6 text-black">
      <section className="relative overflow-hidden rounded-4xl bg-gradient-to-br from-[#f8fbff] via-white to-[#e9fbf7] px-6 py-10 text-black shadow-[0px_20px_60px_-30px_rgba(15,23,42,0.35)] sm:px-10 sm:py-12">
        <div className="pointer-events-none absolute -right-20 -top-20 h-72 w-72 rounded-full bg-[#7dd3fc] opacity-30 blur-3xl" />
        <div className="pointer-events-none absolute -left-24 top-24 h-64 w-64 rounded-full bg-[#34d399] opacity-20 blur-3xl" />
        <div className="relative z-1 flex flex-col items-center gap-3 text-center">
          <p className="text-xs uppercase tracking-[0.3em] text-gray-400">Kaspa</p>
          <h1 className="text-3xl font-bold sm:text-4xl uppercase tracking-[0.3em]">ANALYTICS</h1>
          <p className="max-w-3xl text-sm text-gray-500">
            Network analytics summarize real-time block flow, difficulty, fees, and issuance in one place.
          </p>
        </div>
        <div className="mt-6 grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
          <div className="rounded-3xl bg-gray-50 p-4">
            <div className="flex items-center justify-between">
              <span className="text-sm text-gray-500">Network hashrate</span>
              <FlashOn className="h-5 w-5 text-gray-400" />
            </div>
            <div className="mt-2 text-2xl font-semibold">
              {hashrateDisplay.value} <span className="text-base text-gray-500">{hashrateDisplay.unit}</span>
            </div>
            <div className="mt-2 h-2 w-full rounded-full bg-white">
              <div
                className="h-2 rounded-full"
                style={{
                  width: `${Math.min(100, Math.max(0, hashratePercent))}%`,
                  backgroundColor: "#70C7BA",
                }}
              />
            </div>
          </div>
          <div className="rounded-3xl bg-gray-50 p-4">
            <div className="flex items-center justify-between">
              <span className="text-sm text-gray-500">Network difficulty</span>
              <BarChart className="h-5 w-5 text-gray-400" />
            </div>
            <div className="mt-2 text-2xl font-semibold">
              {difficultyDisplay.value} <span className="text-base text-gray-500">{difficultyDisplay.unit}</span>
            </div>
            <div className="mt-2 h-2 w-full rounded-full bg-white">
              <div
                className="h-2 rounded-full"
                style={{
                  width: `${Math.min(100, Math.max(0, difficultyPercent))}%`,
                  backgroundColor: "#70C7BA",
                }}
              />
            </div>
          </div>
          <div className="rounded-3xl bg-gray-50 p-4">
            <div className="flex items-center justify-between">
              <span className="text-sm text-gray-500">Average block time</span>
              <Swap className="h-5 w-5 text-gray-400" />
            </div>
            <div className="mt-2 text-2xl font-semibold">
              {numeral(avgBlockTime).format("0.0")}
              <span className="text-base text-gray-500"> BPS</span>
            </div>
          </div>
          <div className="rounded-3xl bg-gray-50 p-4">
            <div className="flex items-center justify-between">
              <span className="text-sm text-gray-500">Average transactions</span>
              <ArrowRight className="h-5 w-5 text-gray-400" />
            </div>
            <div className="mt-2 text-2xl font-semibold">
              {numeral(avgTxRate).format("0.0")} <span className="text-base text-gray-500">TPS</span>
            </div>
          </div>
        </div>
      </section>

      <section className="grid grid-cols-1 gap-6 lg:grid-cols-3">
        <div className="rounded-4xl bg-white p-6">
          <div className="flex items-center justify-between">
            <h2 className="text-lg font-semibold">Supply & issuance</h2>
            <Coins className="h-5 w-5 text-gray-400" />
          </div>
          <div className="mt-4 space-y-3">
            <div className="flex items-center justify-between">
              <span className="text-sm text-gray-500">Circulating supply</span>
              <span className="font-medium">
                {isLoadingCoinSupply ? "--" : numeral(circulatingSupply).format("0,0")} KAS
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm text-gray-500">Mined</span>
              <span className="font-medium">{isLoadingCoinSupply ? "--" : numeral(minedPercent).format("0.00")}%</span>
            </div>
            <div className="h-2 w-full rounded-full bg-gray-100">
              <div
                className="h-2 rounded-full"
                style={{ width: `${Math.min(100, minedPercent)}%`, backgroundColor: "#70C7BA" }}
              />
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm text-gray-500">Block reward</span>
              <span className="font-medium">
                {isLoadingBlockReward ? "--" : numeral(blockReward?.blockreward || 0).format("0.000")} KAS
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm text-gray-500">Next reduction</span>
              <span className="font-medium">{isLoadingHalving ? "--" : halving?.nextHalvingDate || "--"}</span>
            </div>
          </div>
        </div>

        <div className="rounded-4xl bg-white p-6">
          <div className="flex items-center justify-between">
            <h2 className="text-lg font-semibold">Fees</h2>
            <Reward className="h-5 w-5 text-gray-400" />
          </div>
          <div className="mt-4 space-y-3">
            <div className="flex items-center justify-between">
              <span className="text-sm text-gray-500">Regular fee</span>
              <span className="font-medium">
                {isLoadingFee || !Number.isFinite(regularFee) ? "--" : `${numeral(regularFee).format("0.00000000")} KAS`}
              </span>
            </div>
          </div>
        </div>

        <div className="rounded-4xl bg-white p-6">
          <div className="flex items-center justify-between">
            <h2 className="text-lg font-semibold">Mempool</h2>
            <PieChart className="h-5 w-5 text-gray-400" />
          </div>
          <div className="mt-4 space-y-4">
            <div className="flex items-center justify-between">
              <span className="text-sm text-gray-500">Mempool size</span>
              <span className="font-medium">{mempoolSize}</span>
            </div>
            <div className="h-2 w-full rounded-full bg-gray-100">
              <div
                className="h-2 rounded-full"
                style={{
                  width: `${Math.min(100, Math.max(0, mempoolPercent))}%`,
                  backgroundColor: "#70C7BA",
                }}
              />
            </div>
          </div>
        </div>
      </section>

      <section className="grid grid-cols-1 gap-6 lg:grid-cols-2">
        <div className="rounded-4xl bg-white p-6">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <h2 className="text-lg font-semibold">Latest blocks</h2>
              <button
                type="button"
                onClick={() => setHideCoinbaseOnlyBlocks((prev) => !prev)}
                className="flex items-center gap-2 rounded-full border px-3 py-1 text-xs font-medium transition hover:bg-gray-50"
                style={{ borderColor: "#70C7BA", backgroundColor: "transparent", color: "#70C7BA" }}
              >
                {hideCoinbaseOnlyBlocks ? "Only wallet-to-wallet" : "Include coinbase-only"}
              </button>
              <button
                type="button"
                aria-label={pauseBlocks ? "Resume latest blocks" : "Pause latest blocks"}
                onClick={() => setPauseBlocks((prev) => !prev)}
                className={`flex h-8 w-8 items-center justify-center rounded-full border text-sm transition ${
                  pauseBlocks
                    ? "border-emerald-200 bg-emerald-100 text-emerald-700"
                    : "border-gray-200 bg-white text-gray-500 hover:bg-gray-50"
                }`}
              >
                {pauseBlocks ? "▶" : "⏸"}
              </button>
            </div>
            <Box className="h-5 w-5 text-gray-400" />
          </div>
          <PageTable
            className="mt-4 text-black table-fixed w-full"
            headers={["Time", "Hash", "BlueScore", "TXs"]}
            additionalClassNames={{
              0: "w-24 whitespace-nowrap",
              1: "overflow-hidden w-72",
              2: "w-20 whitespace-nowrap",
              3: "w-12 whitespace-nowrap",
            }}
            rowClassName={(index) => (index % 2 === 1 ? "bg-gray-25" : "")}
            rows={Array.from({ length: 10 }).map((_, index) => {
              const block = displayedBlocks[index];
              if (!block) return ["--:--:--", "", "", ""];
              return [
                dayjs(parseInt(block.timestamp)).format("HH:mm:ss"),
                <KasLink linkType="block" link to={block.block_hash} mono />,
                block.blueScore,
                block.txCount,
              ];
            })}
          />
        </div>
        <div className="rounded-4xl bg-white p-6">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <h2 className="text-lg font-semibold">Latest transactions</h2>
              <button
                type="button"
                onClick={() => setHideCoinbaseTxs((prev) => !prev)}
                className="flex items-center gap-2 rounded-full border px-3 py-1 text-xs font-medium transition hover:bg-gray-50"
                style={{ borderColor: "#70C7BA", backgroundColor: "transparent", color: "#70C7BA" }}
              >
                {hideCoinbaseTxs ? "Only wallet-to-wallet" : "Include coinbase"}
              </button>
              <button
                type="button"
                aria-label={pauseTransactions ? "Resume latest transactions" : "Pause latest transactions"}
                onClick={() => setPauseTransactions((prev) => !prev)}
                className={`flex h-8 w-8 items-center justify-center rounded-full border text-sm transition ${
                  pauseTransactions
                    ? "border-emerald-200 bg-emerald-100 text-emerald-700"
                    : "border-gray-200 bg-white text-gray-500 hover:bg-gray-50"
                }`}
              >
                {pauseTransactions ? "▶" : "⏸"}
              </button>
            </div>
            <AnalyticsIcon className="h-5 w-5 text-gray-400" />
          </div>
          <PageTable
            className="mt-4 text-black table-fixed w-full"
            headers={["Time", "Transaction ID", "Amount"]}
            additionalClassNames={{
              0: "w-24 whitespace-nowrap",
              1: "overflow-hidden w-72",
              2: "whitespace-nowrap w-36",
            }}
            rowClassName={(index) => (index % 2 === 1 ? "bg-gray-25" : "")}
            rows={Array.from({ length: 10 }).map((_, index) => {
              const transaction = displayedTransactions[index];
              if (!transaction) return ["--:--:--", "", ""];
              return [
                transaction.timestamp ? dayjs(parseInt(transaction.timestamp)).format("HH:mm:ss") : "--:--:--",
                <KasLink linkType="transaction" link to={transaction.txId} mono />,
                <>
                  {numeral(
                    transaction.outputs.reduce((acc, output) => acc + Number(output[1]), 0) / 1_0000_0000,
                  ).format("0,0.[00]")}
                  <span className="text-gray-500 text-nowrap"> KAS</span>
                </>,
              ];
            })}
          />
        </div>
      </section>
      <FooterHelper icon={AnalyticsIcon}>
        <span>
          Network analytics summarize real-time block flow, difficulty, fees, and issuance in one place.
        </span>
      </FooterHelper>
    </div>
  );
}
