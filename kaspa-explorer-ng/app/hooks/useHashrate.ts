import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { API_BASE } from "../api/config";

interface HashrateInfo {
  hashrate: number;
}

export const useHashrate = () =>
  useQuery({
    queryKey: ["hashrate"],
    queryFn: async () => {
      const { data } = await axios.get(`${API_BASE}/info/hashrate`);
      return data as HashrateInfo;
    },
    refetchInterval: 20000,
  });
