import IconMessageBox from "../IconMessageBox";
import KasLink from "../KasLink";
import PageTable from "../PageTable";
import Spinner from "../Spinner";
import Card from "../layout/Card";
import FooterHelper from "../layout/FooterHelper";
import Box from "../assets/box.svg";
import { useBlockdagInfo } from "../hooks/useBlockDagInfo";
import { useSocketRoom } from "../hooks/useSocketRoom";
import { API_BASE } from "../api/config";
import type { Route } from "./+types/miners";
import axios from "axios";
import dayjs from "dayjs";
import localeData from "dayjs/plugin/localeData";
import localizedFormat from "dayjs/plugin/localizedFormat";
import relativeTime from "dayjs/plugin/relativeTime";
import numeral from "numeral";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

dayjs().locale("en");
dayjs.extend(relativeTime);
dayjs.extend(localeData);
dayjs.extend(localizedFormat);

const INITIAL_BLOCK_SAMPLE = 40;
const stripKaspaPrefix = (value: string) => {
  if (value.startsWith("kaspa:")) return value.slice("kaspa:".length);
  if (value.startsWith("kaspatest:")) return value.slice("kaspatest:".length);
  return value;
};

export function meta({}: Route.LoaderArgs) {
  return [
    { title: "Kaspa Miners | Kaspa Explorer" },
    {
      name: "description",
      content: "Explore miner info analytics for recent Kaspa blocks.",
    },
    { name: "keywords", content: "Kaspa miners, miner info, analytics, blocks" },
  ];
}

export default function Miners() {
  const [searchQuery, setSearchQuery] = useState("");
  const { data: blockDagInfo } = useBlockdagInfo();
  const [miners, setMiners] = useState<
    Array<{
      minerInfo: string | null;
      minerAddress: string | null;
      blocks: number;
      lastBlockTime: number | null;
      lastBlockHash: string | null;
    }>
  >([]);
  const [scannedBlocks, setScannedBlocks] = useState(0);
  const [windowStart, setWindowStart] = useState<number | null>(null);
  const [windowEnd, setWindowEnd] = useState<number | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isError, setIsError] = useState(false);
  const [errorCount, setErrorCount] = useState(0);
  const minerMapRef = useRef(
    new Map<
      string,
      {
        minerInfo: string | null;
        minerAddress: string | null;
        blocks: number;
        lastBlockTime: number | null;
        lastBlockHash: string | null;
      }
    >(),
  );

  const upsertMiner = useCallback(
    ({
      minerInfo,
      minerAddress,
      blockTime,
      blockHash,
    }: {
      minerInfo: string | null;
      minerAddress: string | null;
      blockTime: number | null;
      blockHash: string;
    }) => {
      const key = minerAddress || minerInfo || "unknown";
      const existing = minerMapRef.current.get(key);
      if (!existing) {
        minerMapRef.current.set(key, {
          minerInfo,
          minerAddress,
          blocks: 1,
          lastBlockTime: blockTime,
          lastBlockHash: blockHash,
        });
      } else {
        existing.blocks += 1;
        if (blockTime && (!existing.lastBlockTime || blockTime > existing.lastBlockTime)) {
          existing.lastBlockTime = blockTime;
          existing.lastBlockHash = blockHash;
          existing.minerInfo = minerInfo;
        }
      }
    },
    [],
  );

  const handleIncomingBlock = useCallback(async (block: { block_hash: string; timestamp?: string }) => {
    try {
      const { data } = await axios.get(
        `${API_BASE}/blocks/${block.block_hash}?includeTransactions=true&includeColor=true`,
      );
      const minerInfo = data?.extra?.minerInfo ?? null;
      const minerAddress = data?.extra?.minerAddress ?? null;
      const blockTime = Number(data?.header?.timestamp ?? block.timestamp ?? 0) || null;
      upsertMiner({ minerInfo, minerAddress, blockTime, blockHash: block.block_hash });

      setScannedBlocks((prev) => prev + 1);
      if (blockTime) {
        setWindowStart((prev) => (prev ? Math.min(prev, blockTime) : blockTime));
        setWindowEnd((prev) => (prev ? Math.max(prev, blockTime) : blockTime));
      }
      setMiners(Array.from(minerMapRef.current.values()).sort((a, b) => b.blocks - a.blocks));
      setIsLoading(false);
      setIsError(false);
      setErrorCount(0);
    } catch (error) {
      console.error(error);
      setErrorCount((count) => count + 1);
      if (minerMapRef.current.size === 0) {
        setIsError(true);
        setIsLoading(false);
      }
    }
  }, []);

  useSocketRoom({
    room: "blocks",
    eventName: "new-block",
    onMessage: handleIncomingBlock,
  });

  useEffect(() => {
    const tipHash = blockDagInfo?.virtualParentHashes?.[0];
    if (!tipHash || minerMapRef.current.size > 0) return;

    let cancelled = false;

    const fetchInitialMiners = async () => {
      try {
        setIsLoading(true);
        setIsError(false);

        const { data } = await axios.get(`${API_BASE}/blocks`, {
          params: {
            lowHash: tipHash,
            includeBlocks: true,
            includeTransactions: false,
          },
        });

        const hashes: string[] = Array.isArray(data?.blockHashes)
          ? data.blockHashes
          : (data?.blocks ?? [])
              .map((block: any) => block?.verboseData?.hash)
              .filter((hash: string | undefined): hash is string => Boolean(hash));

        const sample = hashes.slice(0, INITIAL_BLOCK_SAMPLE);
        if (!sample.length) {
          throw new Error("No blocks available");
        }

        const blockDetails = await Promise.all(
          sample.map(async (hash) => {
            try {
              const response = await axios.get(`${API_BASE}/blocks/${hash}`, {
                params: { includeTransactions: true, includeColor: true },
              });
              return { hash, data: response.data };
            } catch {
              return null;
            }
          }),
        );

        if (cancelled) return;

        minerMapRef.current.clear();
        let scanned = 0;
        let start: number | null = null;
        let end: number | null = null;

        blockDetails.forEach((entry) => {
          if (!entry) return;
          const { data } = entry;
          const minerInfo = data?.extra?.minerInfo ?? null;
          const minerAddress = data?.extra?.minerAddress ?? null;
          const blockTime = Number(data?.header?.timestamp ?? 0) || null;
          upsertMiner({ minerInfo, minerAddress, blockTime, blockHash: entry.hash });
          scanned += 1;
          if (blockTime) {
            start = start ? Math.min(start, blockTime) : blockTime;
            end = end ? Math.max(end, blockTime) : blockTime;
          }
        });

        setScannedBlocks(scanned);
        setWindowStart(start);
        setWindowEnd(end);
        setMiners(Array.from(minerMapRef.current.values()).sort((a, b) => b.blocks - a.blocks));
        setIsLoading(false);
        setIsError(minerMapRef.current.size === 0);
        setErrorCount(0);
      } catch (error) {
        console.error(error);
        if (cancelled) return;
        setIsLoading(false);
        if (minerMapRef.current.size === 0) {
          setIsError(true);
        }
      }
    };

    fetchInitialMiners();
    return () => {
      cancelled = true;
    };
  }, [blockDagInfo, upsertMiner]);

  const groupedMiners = useMemo(() => {
    const buckets = new Map<string, (typeof miners)[number] & { blocks: number }>();
    miners.forEach((miner) => {
      const address = miner.minerAddress || "unknown";
      const existing = buckets.get(address);
      if (!existing) {
        buckets.set(address, { ...miner });
        return;
      }
      existing.blocks += miner.blocks;
      if (miner.lastBlockTime && (!existing.lastBlockTime || miner.lastBlockTime > existing.lastBlockTime)) {
        existing.lastBlockTime = miner.lastBlockTime;
        existing.lastBlockHash = miner.lastBlockHash;
        existing.minerInfo = miner.minerInfo;
      }
    });
    return Array.from(buckets.values()).sort((a, b) => b.blocks - a.blocks);
  }, [miners]);

  const normalizedQuery = searchQuery.trim().toLowerCase();
  const filteredMiners = useMemo(() => {
    if (!normalizedQuery) return groupedMiners;
    const matching = groupedMiners.filter((miner) => {
      const info = (miner.minerInfo || "").toLowerCase();
      const address = (miner.minerAddress || "").toLowerCase();
      const addressNoPrefix = stripKaspaPrefix(address);
      const queryNoPrefix = stripKaspaPrefix(normalizedQuery);
      return (
        info.includes(normalizedQuery) ||
        address.includes(normalizedQuery) ||
        addressNoPrefix.includes(queryNoPrefix) ||
        addressNoPrefix.includes(normalizedQuery)
      );
    });
    return matching.sort((a, b) => {
      const aAddress = (a.minerAddress || "").toLowerCase();
      const bAddress = (b.minerAddress || "").toLowerCase();
      const aNoPrefix = stripKaspaPrefix(aAddress);
      const bNoPrefix = stripKaspaPrefix(bAddress);
      const queryNoPrefix = stripKaspaPrefix(normalizedQuery);
      const aSuffix = aNoPrefix.endsWith(queryNoPrefix) || aNoPrefix.endsWith(normalizedQuery);
      const bSuffix = bNoPrefix.endsWith(queryNoPrefix) || bNoPrefix.endsWith(normalizedQuery);
      if (aSuffix !== bSuffix) return aSuffix ? -1 : 1;
      return b.blocks - a.blocks;
    });
  }, [groupedMiners, normalizedQuery]);

  if (isLoading) {
    return (
      <div className="flex w-full items-center justify-center py-20">
        <Spinner className="fill-primary h-8 w-8 animate-spin" />
      </div>
    );
  }

  if (isError) {
    return <IconMessageBox icon="error" title="Miner data unavailable" description="Unable to load miner analytics." />;
  }

  const windowStartLabel = windowStart ? dayjs(windowStart).format("MMM D, YYYY HH:mm") : "—";
  const windowEndLabel = windowEnd ? dayjs(windowEnd).format("MMM D, YYYY HH:mm") : "—";
  const totalBlocks = Math.max(1, scannedBlocks);

  return (
    <>
      <section className="relative w-full overflow-hidden rounded-4xl bg-gradient-to-br from-[#f8fbff] via-white to-[#e9fbf7] px-6 py-10 text-black shadow-[0px_20px_60px_-30px_rgba(15,23,42,0.35)] sm:px-10 sm:py-12">
        <div className="pointer-events-none absolute -right-20 -top-20 h-72 w-72 rounded-full bg-[#7dd3fc] opacity-30 blur-3xl" />
        <div className="pointer-events-none absolute -left-24 top-24 h-64 w-64 rounded-full bg-[#34d399] opacity-20 blur-3xl" />
        <div className="relative z-1 mx-auto flex w-full max-w-5xl flex-col items-center text-center">
          <p className="text-xs uppercase tracking-[0.3em] text-gray-400">Kaspa</p>
          <h1 className="max-w-3xl text-3xl font-bold sm:text-4xl uppercase tracking-[0.3em]">Miners</h1>
          <p className="mt-3 max-w-3xl text-sm text-gray-500">
            Miner identities and shares are derived from the recent block window for a high-level activity snapshot.
          </p>
          <div className="mt-6 grid w-full max-w-4xl grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-3 mx-auto">
            <Card title="Scanned blocks" value={numeral(scannedBlocks).format("0,0")} variant="analytics" />
            <Card title="Unique miners" value={numeral(groupedMiners.length).format("0,0")} variant="analytics" />
            <Card title="Window" value={`${windowEndLabel} → ${windowStartLabel}`} variant="analytics" />
          </div>
        </div>
      </section>

      <div className="rounded-4xl bg-white p-6">
        <div className="flex items-center justify-between">
          <h2 className="text-lg font-semibold">Miner info analytics</h2>
          <span className="text-xs uppercase tracking-[0.3em] text-gray-400">Live network snapshot</span>
        </div>
        <p className="mt-2 text-sm text-gray-500">Miner identities extracted from incoming blocks in real time.</p>

        <div className="mt-4 flex flex-col gap-2 rounded-2xl border border-gray-100 bg-gray-25 p-4 sm:flex-row sm:items-center sm:justify-between">
          <div>
            <div className="text-sm font-semibold text-black">Filter miner info</div>
            <div className="text-xs text-gray-500">Type a keyword (e.g. Umbrel) to filter.</div>
          </div>
          <input
            type="text"
            value={searchQuery}
            onChange={(event) => setSearchQuery(event.target.value)}
            placeholder="Search miner info..."
            className="w-full rounded-xl border border-gray-200 bg-white px-4 py-2 text-sm outline-none sm:w-64"
          />
        </div>

        <PageTable
          className="mt-4 text-black table-fixed w-full"
          headers={["Miner info", "Address", "Blocks", "Share", "Last seen"]}
          additionalClassNames={{
            0: "overflow-hidden w-56",
            1: "overflow-hidden w-96 pl-4",
            2: "w-20 whitespace-nowrap pl-4",
            3: "w-20 whitespace-nowrap pl-4",
            4: "w-20 whitespace-nowrap",
          }}
          rowClassName={(index) => (index % 2 === 1 ? "bg-gray-25" : "")}
          rows={filteredMiners.map((miner) => {
            const share = ((miner.blocks / totalBlocks) * 100).toFixed(1);
            const displayInfo = miner.minerInfo ? miner.minerInfo.split(":")[0] : "Unknown";
            return [
              displayInfo,
              miner.minerAddress ? (
                <KasLink linkType="address" link to={miner.minerAddress} mono />
              ) : (
                "—"
              ),
              numeral(miner.blocks).format("0,0"),
              `${share}%`,
              miner.lastBlockTime ? dayjs(miner.lastBlockTime).fromNow() : "—",
            ];
          })}
        />
      </div>
      <FooterHelper icon={Box}>
        <span>Miner identities and shares are derived from the recent block window for a high-level activity snapshot.</span>
      </FooterHelper>
    </>
  );
}
