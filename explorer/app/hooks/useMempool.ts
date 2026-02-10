import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { useCallback, useEffect, useState } from "react";
import { useSocketRoom } from "./useSocketRoom";
import { VPROGS_BASE } from "../api/urls";

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

export const useMempoolSummary = () =>
  useQuery({
    queryKey: ["mempool-summary"],
    queryFn: async () => {
      const vprogsBase = VPROGS_BASE ? VPROGS_BASE.replace(/\/$/, "") : "";
      if (!vprogsBase) return {} as MempoolSummary;
      const { data } = await axios.get(`${vprogsBase}/api/mempool/summary`);
      return data as MempoolSummary;
    },
    refetchInterval: 30000,
  });

export const useMempoolHistory = (limit: number = 120) =>
  useQuery({
    queryKey: ["mempool-history", { limit }],
    queryFn: async () => {
      const vprogsBase = VPROGS_BASE ? VPROGS_BASE.replace(/\/$/, "") : "";
      if (!vprogsBase) return [] as MempoolHistoryPoint[];
      const { data } = await axios.get(`${vprogsBase}/api/mempool/history`, { params: { limit } });
      return (data?.history || []) as MempoolHistoryPoint[];
    },
    refetchInterval: 60000,
  });

export const useMempoolLive = (windowSeconds: number = 60) => {
  const [summary, setSummary] = useState<MempoolSummary>({});
  const [history, setHistory] = useState<MempoolHistoryPoint[]>([]);
  const [isConnecting, setIsConnecting] = useState(true);

  const handleMessage = useCallback(
    (payload: MempoolSummary) => {
      setIsConnecting(false);
      const capturedAtMs = payload.capturedAt ? Date.parse(payload.capturedAt) : Date.now();
      setSummary(payload);
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

  useSocketRoom<MempoolSummary>({
    room: "mempool-live",
    eventName: "mempool-live",
    onMessage: handleMessage,
  });

  useEffect(() => {
    setIsConnecting(true);
  }, [windowSeconds]);

  return { summary, history, isConnecting };
};
