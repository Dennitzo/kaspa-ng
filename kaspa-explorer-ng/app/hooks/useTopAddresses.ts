import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { getApiBase } from "../api/config";

export const useTopAddresses = () =>
  useQuery({
    queryKey: ["topAddresses"],
    queryFn: async () => {
      const { data } = await axios.get(`${getApiBase()}/addresses/top`);
      return data[0] as TopAddresses;
    },
  });

interface TopAddresses {
  timestamp: number;
  ranking: {
    rank: number;
    address: string;
    amount: number;
  }[];
}
