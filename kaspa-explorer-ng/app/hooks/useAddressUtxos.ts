import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { API_BASE } from "../api/config";

export const useAddressUtxos = (address: string) =>
  useQuery({
    queryKey: ["utxos", { address }],
    queryFn: async () => {
      const { data } = await axios.get(`${API_BASE}/addresses/${address}/utxos`);
      return data.sort(
        (a: Utxo, b: Utxo) => Number(b.utxoEntry.blockDaaScore) - Number(a.utxoEntry.blockDaaScore),
      ) as Utxo[];
    },
  });

interface Utxo {
  address: string;
  outpoint: {
    transactionId: string;
    index: number;
  };
  utxoEntry: {
    amount: string;
    scriptPublicKey: {
      scriptPublicKey: string;
    };
    blockDaaScore: string;
    isCoinbase: boolean;
  };
}
