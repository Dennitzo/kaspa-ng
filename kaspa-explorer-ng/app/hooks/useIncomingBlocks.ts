import { useSocketRoom } from "./useSocketRoom";
import { useBlockdagInfo } from "./useBlockDagInfo";
import { useSocketConnected } from "../api/socket";
import axios from "axios";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

export interface Block {
  block_hash: string;
  difficulty: number;
  blueScore: string;
  timestamp: string;
  txCount: number;
  txs: {
    txId: string;
    outputs: [string, string][];
    timestamp?: string;
  }[];
}

const MAX_BLOCK_BUFFER = 200;
const MAX_TX_BUFFER = 200;
const API_BASE = "https://api.kaspa.org";

export const useIncomingBlocks = () => {
  const { connected } = useSocketConnected();
  const { data: blockDagInfo } = useBlockdagInfo();
  const [blocks, setBlocks] = useState<Block[]>([]);

  const startTime = useMemo(() => Date.now() + 500, []);
  const [blockCount, setBlockCount] = useState(0);
  const [avgBlockTime, setAvgBlockTime] = useState(0);
  const [txCount, setTxCount] = useState(0);
  const [avgTxRate, setAvgTxRate] = useState(0);

  const handleBlocks = useCallback((newBlock: Block) => {
    setBlockCount((prevBlockCount) => prevBlockCount + 1);
    setTxCount((prevTxCount) => prevTxCount + (newBlock.txCount || 0));
    setBlocks((prevBlocks) => [newBlock, ...prevBlocks.slice(0, MAX_BLOCK_BUFFER - 1)]);
  }, []);
  const lastThrottleTime = useRef(0);

  useSocketRoom<Block>({
    room: "blocks",
    eventName: "new-block",
    onMessage: handleBlocks,
  });

  useEffect(() => {
    if (connected) return;
    const tipHash = blockDagInfo?.virtualParentHashes?.[0];
    if (!tipHash) return;

    let cancelled = false;
    const fetchBlocks = async () => {
      try {
        const { data } = await axios.get(`${API_BASE}/blocks`, {
          params: {
            lowHash: tipHash,
            includeBlocks: true,
            includeTransactions: true,
          },
        });

        const nextBlocks = (data?.blocks ?? [])
          .map((block: any): Block | null => {
            const hash = block?.verboseData?.hash;
            if (!hash) return null;

            const timestamp = block?.header?.timestamp ?? block?.verboseData?.blockTime ?? "0";
            const txs = Array.isArray(block?.transactions)
              ? block.transactions
                  .map((tx: any) => {
                    const txId = tx?.verboseData?.transactionId ?? tx?.transaction_id ?? "";
                    if (!txId) return null;
                    const outputs = Array.isArray(tx?.outputs)
                      ? tx.outputs.map((output: any) => [
                          output?.verboseData?.scriptPublicKeyAddress ?? "",
                          String(output?.amount ?? 0),
                        ])
                      : [];
                    return { txId, outputs, timestamp: String(timestamp) };
                  })
                  .filter(Boolean)
              : [];

            return {
              block_hash: hash,
              difficulty: Number(block?.verboseData?.difficulty ?? 0),
              blueScore: String(block?.header?.blueScore ?? block?.verboseData?.blueScore ?? ""),
              timestamp: String(timestamp),
              txCount: block?.verboseData?.transactionIds?.length ?? txs.length,
              txs,
            };
          })
          .filter((block: Block | null): block is Block => block !== null)
          .sort((a, b) => Number(b.timestamp) - Number(a.timestamp));

        if (cancelled) return;

        setBlocks(nextBlocks.slice(0, MAX_BLOCK_BUFFER));

        if (nextBlocks.length >= 2) {
          const newest = Number(nextBlocks[0].timestamp);
          const oldest = Number(nextBlocks[nextBlocks.length - 1].timestamp);
          const elapsed = Math.max((newest - oldest) / 1000, 1);
          const totalTxs = nextBlocks.reduce((sum, block) => sum + (block.txCount || 0), 0);
          setAvgBlockTime(nextBlocks.length / elapsed);
          setAvgTxRate(totalTxs / elapsed);
        }
      } catch {
        // ignore; socket might still connect later
      }
    };

    fetchBlocks();
    const intervalId = setInterval(fetchBlocks, 10000);

    return () => {
      cancelled = true;
      clearInterval(intervalId);
    };
  }, [connected, blockDagInfo?.virtualParentHashes?.[0]]);

  useEffect(() => {
    // Throttling logic with `useRef`
    const now = Date.now();
    if (now - lastThrottleTime.current >= 200) {
      const elapsedSeconds = Math.max(now - startTime, 500) / 1000;
      setAvgBlockTime(() => blockCount / elapsedSeconds);
      setAvgTxRate(() => txCount / elapsedSeconds);
      lastThrottleTime.current = now;
    }
  }, [blockCount, startTime, txCount]);

  const txs: { txId: string; outputs: [string, string][]; timestamp?: string }[] = [];

  for (const block of blocks) {
    for (const tx of block.txs) {
      txs.push({ ...tx, timestamp: block.timestamp });
      if (txs.length > MAX_TX_BUFFER) break;
    }
    if (txs.length > MAX_TX_BUFFER) break;
  }

  return {
    blocks,
    avgBlockTime,
    avgTxRate,
    transactions: txs,
  };
};
