import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { getApiBase, getNetworkId } from "../api/config";

interface CoinSupplyInfo {
  circulatingSupply: number;
  maxSupply: number;
}

export const useCoinSupply = () =>
  useQuery({
    queryKey: ["coinSupply", getNetworkId(), getApiBase()],
    queryFn: async () => {
      const { data } = await axios.get(`${getApiBase()}/info/coinsupply`);
      return data as CoinSupplyInfo;
    },
    refetchInterval: 60000,
  });
