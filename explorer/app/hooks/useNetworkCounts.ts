import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { VPROGS_BASE } from "../api/urls";

export interface NetworkCountsResponse {
  totalTransactions?: number;
  numberOfAddresses?: number;
  totalFeesSompi?: number;
  updatedAt?: string;
}

export const useNetworkCounts = () =>
  useQuery({
    queryKey: ["network-counts"],
    queryFn: async () => {
      const vprogsBase = VPROGS_BASE ? VPROGS_BASE.replace(/\/$/, "") : "";
      if (!vprogsBase) return {} as NetworkCountsResponse;
      const { data } = await axios.get(`${vprogsBase}/api/address-metrics`);
      return {
        totalTransactions: data?.totalTransactions,
        numberOfAddresses: data?.numberOfAddresses,
        totalFeesSompi: data?.totalFeesSompi,
        updatedAt: data?.updatedAt,
      } as NetworkCountsResponse;
    },
    refetchInterval: 600000,
  });
