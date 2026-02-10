import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { apiUrl } from "../api/urls";

interface CoinSupplyInfo {
  circulatingSupply: number;
  maxSupply: number;
}

export const useCoinSupply = () =>
  useQuery({
    queryKey: ["coinSupply"],
    queryFn: async () => {
      const { data } = await axios.get(apiUrl("info/coinsupply"));
      return data as CoinSupplyInfo;
    },
    refetchInterval: 60000,
  });
