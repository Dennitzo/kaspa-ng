import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { apiUrl } from "../api/urls";

interface BlockRewardInfo {
  blockreward: number;
}

export const useBlockReward = () =>
  useQuery({
    staleTime: 60000,
    queryKey: ["blockReward"],
    queryFn: async () => {
      const { data } = await axios.get(apiUrl("info/blockreward"));
      return data as BlockRewardInfo;
    },
  });
