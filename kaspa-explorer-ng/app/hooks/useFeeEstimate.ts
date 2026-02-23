import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { getApiBase, getNetworkId } from "../api/config";

export const useFeeEstimate = () =>
  useQuery({
    queryKey: ["fee-estimate", getNetworkId(), getApiBase()],
    queryFn: async () => {
      const { data } = await axios.get(`${getApiBase()}/info/fee-estimate`);
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
