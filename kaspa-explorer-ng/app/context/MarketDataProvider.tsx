import { getMarketData } from "../api/kaspa-api-client";
import numeral from "numeral";
import { createContext, useEffect, useState } from "react";

interface MarketData {
  price: number | undefined;
  change24h: string | undefined;
}

export const MarketDataContext = createContext<MarketData | undefined>(undefined);

export const MarketDataProvider = ({ children }: { children: React.ReactNode }) => {
  const [marketData, setMarketData] = useState<MarketData>({
    price: undefined,
    change24h: undefined,
  });

  const updateMarketData = async () => {
    try {
      const marketDataResp = await getMarketData();
      const price = marketDataResp?.current_price?.usd;
      const change24hRaw = marketDataResp?.price_change_percentage_24h;
      setMarketData({
        price: typeof price === "number" ? price : undefined,
        change24h: typeof change24hRaw === "number" ? numeral(change24hRaw).format("+0.00") : undefined,
      });
    } catch {
      // Price endpoint may be unavailable on non-mainnet/self-hosted setups.
      setMarketData((current) =>
        current.price === undefined && current.change24h === undefined
          ? current
          : { price: undefined, change24h: undefined },
      );
    }
  };

  useEffect(() => {
    updateMarketData();
    const updateInterval = setInterval(updateMarketData, 60_000);
    return () => {
      clearInterval(updateInterval);
    };
  }, []);

  return <MarketDataContext.Provider value={marketData}>{children}</MarketDataContext.Provider>;
};
