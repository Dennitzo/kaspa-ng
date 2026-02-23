import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { getApiBase, getNetworkId } from "../api/config";

interface HalvingInfo {
  nextHalvingTimestamp: number;
  nextHalvingDate: string;
  nextHalvingAmount: number;
}

export const useHalving = () =>
  useQuery({
    queryKey: ["halving", getNetworkId(), getApiBase()],
    queryFn: async () => {
      const { data } = await axios.get(`${getApiBase()}/info/halving`);
      return data as HalvingInfo;
    },
  });
