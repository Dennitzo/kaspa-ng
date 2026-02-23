import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { getApiBase } from "../api/config";

export const useAddressBalance = (address: string) =>
  useQuery({
    queryKey: ["addresses", { address }],
    queryFn: async () => {
      const { data } = await axios.get(`${getApiBase()}/addresses/${address}/balance`);
      return data as AddressBalance;
    },
  });

export interface AddressBalance {
  address: string;
  balance: number;
}
