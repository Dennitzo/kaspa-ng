import { useSocketRoom } from "./useSocketRoom";
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

export const useIncomingBlocks = () => {
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
