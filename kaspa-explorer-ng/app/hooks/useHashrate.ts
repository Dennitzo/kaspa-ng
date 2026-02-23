import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { getApiBase, getNetworkId } from "../api/config";

interface HashrateInfo {
  hashrate: number;
}

export const useHashrate = () =>
  useQuery({
    queryKey: ["hashrate", getNetworkId(), getApiBase()],
    queryFn: async () => {
      const { data } = await axios.get(`${getApiBase()}/info/hashrate`);
      return data as HashrateInfo;
    },
    refetchInterval: 20000,
  });
