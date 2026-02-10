import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { apiUrl, VPROGS_BASE } from "../api/urls";

export interface BalanceFlowPoint {
  timestamp: number;
  balance: number;
}

export interface BalanceFlowResponse {
  points: BalanceFlowPoint[];
  hash?: string;
  generatedAt?: string;
  cached?: boolean;
  source?: string;
  pending?: boolean;
}

export interface BalanceFlowLatestResponse {
  jobType?: string;
  runAt?: string;
  durationMs?: number;
  outputSize?: number;
  hash?: string;
  targetAddress?: string;
  requestLimit?: number;
  points?: BalanceFlowPoint[];
  error?: string;
}

export const useAddressBalanceFlow = (
  address: string,
  limit: number = 10000,
  refetchInterval: number | false = false,
  enabled: boolean = true,
) =>
  useQuery({
    queryKey: ["balance-flow", { address, limit }],
    queryFn: async () => {
      const vprogsBase = VPROGS_BASE ? VPROGS_BASE.replace(/\/$/, "") : "";
      const url = vprogsBase ? `${vprogsBase}/api/balance-flow` : apiUrl(`addresses/${address}/balance-flow`);
      const params = vprogsBase ? { address, limit } : { limit };
      const response = await axios.get(url, { params });
      return response.data as BalanceFlowResponse;
    },
    enabled: !!address && enabled,
    refetchInterval,
    keepPreviousData: true,
  });

export const useAddressBalanceFlowLatest = (
  address: string,
  limit: number = 10000,
  refetchInterval: number | false = false,
  enabled: boolean = true,
) =>
  useQuery({
    queryKey: ["balance-flow-latest", { address, limit }],
    queryFn: async () => {
      const vprogsBase = VPROGS_BASE ? VPROGS_BASE.replace(/\/$/, "") : "";
      const url = vprogsBase
        ? `${vprogsBase}/api/balance-flow`
        : apiUrl(`addresses/${address}/balance-flow/latest`);
      const params = vprogsBase ? { address, limit } : { limit };
      const response = await axios.get(url, { params });
      return response.data as BalanceFlowLatestResponse;
    },
    enabled: !!address && enabled,
    refetchInterval,
  });
