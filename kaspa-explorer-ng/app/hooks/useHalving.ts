import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { API_BASE } from "../api/config";

interface HalvingInfo {
  nextHalvingTimestamp: number;
  nextHalvingDate: string;
  nextHalvingAmount: number;
}

export const useHalving = () =>
  useQuery({
    queryKey: ["halving"],
    queryFn: async () => {
      const { data } = await axios.get(`${API_BASE}/info/halving`);
      return data as HalvingInfo;
    },
  });
