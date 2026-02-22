import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { API_BASE } from "../api/config";

interface BlockRewardInfo {
  blockreward: number;
}

export const useBlockReward = () =>
  useQuery({
    staleTime: 60000,
    queryKey: ["blockReward"],
    queryFn: async () => {
      const { data } = await axios.get(`${API_BASE}/info/blockreward`);
      return data as BlockRewardInfo;
    },
  });
