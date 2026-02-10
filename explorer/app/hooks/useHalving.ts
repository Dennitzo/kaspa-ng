import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { apiUrl } from "../api/urls";

interface HalvingInfo {
  nextHalvingTimestamp: number;
  nextHalvingDate: string;
  nextHalvingAmount: number;
}

export const useHalving = () =>
  useQuery({
    queryKey: ["halving"],
    queryFn: async () => {
      const { data } = await axios.get(apiUrl("info/halving"));
      return data as HalvingInfo;
    },
  });
