import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { API_BASE } from "../api/config";

export const useFeeEstimate = () =>
  useQuery({
    queryKey: ["fee-estimate"],
    queryFn: async () => {
      const { data } = await axios.get(`${API_BASE}/info/fee-estimate`);
      return data as FeeEstimate;
    },
    retry: false,
  });

interface FeeBucket {
  feerate: number;
  estimateSeconds: number;
}

interface FeeEstimate {
  priorityBucket: FeeBucket;
  normalBuckets: FeeBucket[];
  lowBuckets: FeeBucket[];
}
