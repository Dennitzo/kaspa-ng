import { getApiBase } from "./config";

const DEFAULT_HEADERS = {
  "Access-Control-Allow-Origin": "*",
  "Cache-Control": "no-cache",
};

const MAINNET_MARKET_API_BASE = "https://api.kaspa.org";

type MarketDataOptions = {
  // In testnet mode we intentionally display the mainnet market feed.
  preferMainnetSource?: boolean;
};

export async function getMarketData(options?: MarketDataOptions) {
  const currentBase = getApiBase();
  const orderedBases = options?.preferMainnetSource
    ? [MAINNET_MARKET_API_BASE, currentBase]
    : [currentBase, MAINNET_MARKET_API_BASE];
  const bases = [...new Set(orderedBases)];

  let lastStatus: number | null = null;
  for (const base of bases) {
    const response = await fetch(`${base}/info/market-data`, {
      headers: DEFAULT_HEADERS,
    });
    if (response.ok) {
      return response.json();
    }
    lastStatus = response.status;
  }

  throw new Error(`market-data request failed (${lastStatus ?? "unknown"})`);
}

//   const res = await fetch(`${getApiBase()}blocks/${hash}?includeColor=true`, {
//     headers: { "Access-Control-Allow-Origin": "*" },
//   })
//     .then((response) => response.json())
//     .then((data) => {
//       return data;
//     });
//   return res;
// }
//
// export async function getTransaction(hash, blockHash) {
//   const queryParams = blockHash ? `?blockHash=${blockHash}` : "";
//   const res = await fetch(`${getApiBase()}transactions/${hash}${queryParams}`, {
//     headers: { "Access-Control-Allow-Origin": "*" },
//   })
//     .then((response) => response.json())
//     .then((data) => {
//       return data;
//     });
//   return res;
// }
//
// export async function getBlockdagInfo() {
//   const res = await fetch(`${getApiBase()}info/blockdag`, {
//     headers: { "Access-Control-Allow-Origin": "*" },
//   })
//     .then((response) => response.json())
//     .then((data) => {
//       return data;
//     });
//   return res;
// }
//
// export async function getKaspadInfo() {
//   const res = await fetch(`${getApiBase()}info/kaspad`, {
//     headers: { "Access-Control-Allow-Origin": "*" },
//   })
//     .then((response) => response.json())
//     .then((data) => {
//       return data;
//     });
//   return res;
// }
//
// export async function getHashrate() {
//   const res = await fetch(`${getApiBase()}info/hashrate`, {
//     headers: { "Access-Control-Allow-Origin": "*" },
//   })
//     .then((response) => response.json())
//     .then((data) => {
//       return data;
//     });
//   return res;
// }
//
// export async function getHashrateMax() {
//   const res = await fetch(`${getApiBase()}info/hashrate/max`, {
//     headers: { "Access-Control-Allow-Origin": "*" },
//   })
//     .then((response) => response.json())
//     .then((data) => {
//       return data;
//     });
//   return res;
// }
//
// export async function getFeeEstimate() {
//   const res = await fetch(`${getApiBase()}info/fee-estimate`, {
//     headers: { "Access-Control-Allow-Origin": "*" },
//   })
//     .then((response) => response.json())
//     .then((data) => {
//       return data;
//     });
//   return res;
// }
//
// export async function getCoinSupply() {
//   const res = await fetch(`${getApiBase()}info/coinsupply`, {
//     headers: { "Access-Control-Allow-Origin": "*" },
//   })
//     .then((response) => response.json())
//     .then((data) => {
//       return data;
//     });
//   return res;
// }
//
// export async function getAddressBalance(addr) {
//   const res = await fetch(`${getApiBase()}addresses/${addr}/balance`, {
//     headers: { "Access-Control-Allow-Origin": "*" },
//   })
//     .then((response) => response.json())
//     .then((data) => {
//       return data.balance;
//     });
//   return res;
// }
//
// export async function getAddressTxCount(addr) {
//   const res = await fetch(`${getApiBase()}addresses/${addr}/transactions-count`, {
//     headers: { "Access-Control-Allow-Origin": "*" },
//   })
//     .then((response) => response.json())
//     .then((data) => {
//       return data;
//     });
//   return res;
// }
//
// export async function getAddressUtxos(addr) {
//   const res = await fetch(`${getApiBase()}addresses/${addr}/utxos`, {
//     headers: { "Access-Control-Allow-Origin": "*" },
//   })
//     .then((response) => response.json())
//     .then((data) => {
//       return data;
//     });
//   return res;
// }
//
// export async function getAddressName(addr) {
//   const res = await fetch(`${getApiBase()}addresses/${addr}/name`, {
//     headers: { "Access-Control-Allow-Origin": "*" },
//   })
//     .then((response) => response.json())
//     .then((data) => {
//       return data;
//     });
//   return res;
// }
//
// export async function getHalving() {
//   const res = await fetch(`${getApiBase()}info/halving`, {
//     headers: { "Access-Control-Allow-Origin": "*" },
//   })
//     .then((response) => response.json())
//     .then((data) => {
//       return data;
//     });
//   return res;
// }
//
// export async function getTransactionsFromAddress(addr, limit = 20, offset = 0) {
//   const res = await fetch(
//     `${getApiBase()}addresses/${addr}/full-transactions?limit=${limit}&offset=${offset}`,
//     {
//       headers: {
//         "Access-Control-Allow-Origin": "*",
//         "content-type": "application/json",
//       },
//       method: "GET",
//     },
//   )
//     .then((response) => response.json())
//     .then((data) => {
//       return data;
//     });
//   return res;
// }
//
// export async function getTransactions(tx_list, inputs, outputs) {
//   const res = await fetch(`${getApiBase()}transactions/search`, {
//     headers: {
//       "Access-Control-Allow-Origin": "*",
//       "content-type": "application/json",
//     },
//     method: "POST",
//     body: JSON.stringify({ transactionIds: tx_list }),
//   })
//     .then((response) => response.json())
//     .then((data) => {
//       return data;
//     });
//   return res;
// }
