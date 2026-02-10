import { useQuery } from "@tanstack/react-query";
import axios from "axios";
import { apiUrl } from "../api/urls";

export const useAddressUtxos = (address: string) =>
  useQuery({
    queryKey: ["utxos", { address }],
    queryFn: async () => {
      const { data } = await axios.get(apiUrl(`addresses/${address}/utxos`));
      return data.sort(
        (a: Utxo, b: Utxo) => Number(b.utxoEntry.blockDaaScore) - Number(a.utxoEntry.blockDaaScore),
      ) as Utxo[];
    },
    refetchInterval: 60000,
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
