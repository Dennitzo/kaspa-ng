import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { API_BASE } from "../api/config";

export const useAddressBalance = (address: string) =>
  useQuery({
    queryKey: ["addresses", { address }],
    queryFn: async () => {
      const { data } = await axios.get(`${API_BASE}/addresses/${address}/balance`);
      return data as AddressBalance;
    },
  });

export interface AddressBalance {
  address: string;
  balance: number;
}
