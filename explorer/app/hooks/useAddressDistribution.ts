import { useQuery } from "@tanstack/react-query";
import { VPROGS_BASE } from "../api/urls";

export interface DistributionTier {
  id: string;
  name: string;
  minKas: number;
  maxKas?: number | null;
  count: number;
  totalSompi: number;
  sharePct?: number | null;
}

export interface DistributionResponse {
  timestamp: number;
  tiers: DistributionTier[];
  updatedAt?: string | null;
  hash?: string | null;
}

const resolveBase = () => {
  if (VPROGS_BASE) return VPROGS_BASE.replace(/\/$/, "");
  if (typeof window !== "undefined") return `http://${window.location.hostname}:19115`;
  return "http://umbrel.local:19115";
};

export const useAddressDistribution = (refetchInterval: number | false = 60000) =>
  useQuery({
    queryKey: ["address-distribution"],
    queryFn: async () => {
      const base = resolveBase();
      const response = await fetch(`${base}/api/address-distribution`, { cache: "no-store" });
      if (!response.ok) {
        const errorPayload = await response.json().catch(() => ({}));
        throw new Error(errorPayload?.error || "distribution fetch failed");
      }
      return (await response.json()) as DistributionResponse;
    },
    refetchInterval,
  });
