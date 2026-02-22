import { useCallback, useEffect, useState } from "react";
import { useSocketRoom } from "./useSocketRoom";

export interface MempoolBucket {
  min: number;
  max?: number | null;
  count: number;
  mass: number;
  fee: number;
}

export interface MempoolTile {
  id?: string | null;
  mass: number;
  fee: number;
  feeRate: number;
  confirmed?: boolean;
}

export interface MempoolAggregates {
  remainingCount?: number;
  remainingMass?: number;
  feeRateMin?: number;
  feeRateMedian?: number;
  feeRateP90?: number;
  feeRateMax?: number;
}

export interface MempoolSummary {
  capturedAt?: string;
  txCount?: number;
  totalMass?: number;
  totalFee?: number;
  feeRateMin?: number;
  feeRateMedian?: number;
  feeRateP90?: number;
  feeRateMax?: number;
  buckets?: MempoolBucket[];
  tiles?: MempoolTile[];
  aggregates?: MempoolAggregates;
  blockMassLimit?: number;
}

export interface MempoolHistoryPoint {
  capturedAt?: string;
  txCount?: number;
  totalMass?: number;
  totalFee?: number;
  feeRateMedian?: number;
  feeRateP90?: number;
}

export const useMempoolLive = (windowSeconds: number = 60) => {
  const [summary, setSummary] = useState<MempoolSummary>({});
  const [history, setHistory] = useState<MempoolHistoryPoint[]>([]);
  const [isConnecting, setIsConnecting] = useState(true);

  const pushHistory = useCallback(
    (payload: MempoolSummary) => {
      const capturedAtMs = payload.capturedAt ? Date.parse(payload.capturedAt) : Date.now();
      setHistory((prev) => {
        const cutoff = capturedAtMs - windowSeconds * 1000;
        const next = [
          {
            capturedAt: payload.capturedAt,
            txCount: payload.txCount,
            totalMass: payload.totalMass,
            totalFee: payload.totalFee,
            feeRateMedian: payload.feeRateMedian,
            feeRateP90: payload.feeRateP90,
          },
          ...prev,
        ];
        return next.filter((point) => {
          if (!point.capturedAt) return true;
          const ts = Date.parse(point.capturedAt);
          return Number.isFinite(ts) && ts >= cutoff;
        });
      });
    },
    [windowSeconds],
  );

  const handleLiveMessage = useCallback(
    (payload: MempoolSummary) => {
      setIsConnecting(false);
      setSummary(payload || {});
      pushHistory(payload || {});
    },
    [pushHistory],
  );

  const handleLegacyCount = useCallback(
    (txCount: number) => {
      if (!Number.isFinite(txCount)) return;
      setIsConnecting(false);
      const payload: MempoolSummary = {
        capturedAt: new Date().toISOString(),
        txCount,
      };
      setSummary((prev) => ({ ...prev, ...payload }));
      pushHistory(payload);
    },
    [pushHistory],
  );

  useSocketRoom<MempoolSummary>({
    room: "mempool-live",
    eventName: "mempool-live",
    onMessage: handleLiveMessage,
  });

  useSocketRoom<number>({
    room: "mempool",
    eventName: "mempool",
    onMessage: handleLegacyCount,
  });

  useEffect(() => {
    setIsConnecting(true);
  }, [windowSeconds]);

  return { summary, history, isConnecting };
};
