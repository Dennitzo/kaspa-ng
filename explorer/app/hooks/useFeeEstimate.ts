import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { apiUrl } from "../api/urls";

export const useFeeEstimate = () =>
  useQuery({
    queryKey: ["fee-estimate"],
    queryFn: async () => {
      const { data } = await axios.get(apiUrl("info/fee-estimate"));
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
