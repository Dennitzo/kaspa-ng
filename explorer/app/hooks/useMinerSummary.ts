import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { apiUrl } from "../api/urls";

export const useMinerSummary = (scanLimit: number = 5000, top: number = 50) =>
  useQuery({
    queryKey: ["miners", { scanLimit, top }],
    queryFn: async () => {
      const { data } = await axios.get(apiUrl("miners/summary"), {
        params: {
          scan_limit: scanLimit,
          top,
        },
      });
      return data as MinerSummaryResponse;
    },
    refetchInterval: 60000,
  });

export interface MinerSummaryItem {
  minerInfo: string | null;
  minerAddress: string | null;
  blocks: number;
  lastBlockTime: number | null;
  lastBlockHash: string | null;
}

export interface MinerSummaryResponse {
  scannedBlocks: number;
  uniqueMiners: number;
  windowStart: number | null;
  windowEnd: number | null;
  miners: MinerSummaryItem[];
}
