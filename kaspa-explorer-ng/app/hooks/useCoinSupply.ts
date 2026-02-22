import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { API_BASE } from "../api/config";

interface CoinSupplyInfo {
  circulatingSupply: number;
  maxSupply: number;
}

export const useCoinSupply = () =>
  useQuery({
    queryKey: ["coinSupply"],
    queryFn: async () => {
      const { data } = await axios.get(`${API_BASE}/info/coinsupply`);
      return data as CoinSupplyInfo;
    },
    refetchInterval: 60000,
  });
