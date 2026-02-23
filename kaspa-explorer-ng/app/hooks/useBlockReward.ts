import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { getApiBase, getNetworkId } from "../api/config";

interface BlockRewardInfo {
  blockreward: number;
}

export const useBlockReward = () =>
  useQuery({
    staleTime: 60000,
    queryKey: ["blockReward", getNetworkId(), getApiBase()],
    queryFn: async () => {
      const { data } = await axios.get(`${getApiBase()}/info/blockreward`);
      return data as BlockRewardInfo;
    },
  });
