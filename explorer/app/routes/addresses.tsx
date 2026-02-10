import KasLink from "../KasLink";
import LoadingMessage from "../LoadingMessage";
import PageTable from "../PageTable";
import { VPROGS_BASE } from "../api/urls";
import AccountBalanceWallet from "../assets/account_balance_wallet.svg";
import { useCoinSupply } from "../hooks/useCoinSupply";
import { useTopAddresses } from "../hooks/useTopAddresses";
import Card from "../layout/Card";
import FooterHelper from "../layout/FooterHelper";
import numeral from "numeral";
import { AddressLabelContext } from "../context/AddressLabelProvider";
import { useContext, useEffect, useMemo, useState } from "react";

export function meta() {
  return [
    { title: "Kaspa Addresses List | Kaspa Explorer" },
    {
      name: "description",
      content: "Browse Kaspa addresses. Track balances, transaction history, and recent activity on the network.",
    },
    { name: "keywords", content: "Kaspa addresses, blockchain explorer, wallet, transaction history, balances" },
  ];
}

export default function Addresses() {
  const { data: topAddresses, isLoading } = useTopAddresses();
  const { data: coinSupply, isLoading: isLoadingSupply } = useCoinSupply();
  const { labels } = useContext(AddressLabelContext);
  const vprogsBaseUrl = VPROGS_BASE;
  const [metrics, setMetrics] = useState<{
    numberOfAddresses?: number;
    top10Pct?: number;
    top100Pct?: number;
    top1000Pct?: number;
  } | null>(null);

  useEffect(() => {
    if (!vprogsBaseUrl) return;
    fetch(`${vprogsBaseUrl}/api/address-metrics`)
      .then((response) => (response.ok ? response.json() : null))
      .then((payload) => {
        if (!payload) return;
        setMetrics({
          numberOfAddresses: payload.numberOfAddresses,
          top10Pct: payload.top10Pct,
          top100Pct: payload.top100Pct,
          top1000Pct: payload.top1000Pct,
        });
      })
      .catch(() => {});
  }, [vprogsBaseUrl]);

  const ranking = Array.isArray(topAddresses?.ranking) ? topAddresses!.ranking : [];
  const circulating = coinSupply?.circulatingSupply ? coinSupply.circulatingSupply / 1_0000_0000 : 0;

  const rows = useMemo(() => {
    if (!ranking.length || !circulating) return [];
    return ranking.slice(0, 100).map((addressInfo) => [
      addressInfo.rank + 1,
      <KasLink linkType="address" link to={addressInfo.address} mono />,
      <span className="text-nowrap">
        {numeral(addressInfo.amount).format("0,0")}
        <span className="text-gray-500 text-nowrap"> KAS</span>
      </span>,
      <>
        {numeral((addressInfo.amount / circulating) * 100).format("0.00")}
        <span className="text-gray-500">&nbsp;%</span>
      </>,
    ]);
  }, [ranking, labels, circulating]);

  if (isLoading || isLoadingSupply || !topAddresses || !coinSupply || rows.length === 0) {
    return <LoadingMessage>Loading addresses</LoadingMessage>;
  }

  const calculateSum = (top: number) => ranking.slice(0, top).reduce((acc, curr) => acc + curr.amount, 0);
  const top10Pct = metrics?.top10Pct ?? (circulating ? (calculateSum(10) / circulating) * 100 : null);
  const top100Pct = metrics?.top100Pct ?? (circulating ? (calculateSum(100) / circulating) * 100 : null);
  const top1000Pct = metrics?.top1000Pct ?? (circulating ? (calculateSum(1000) / circulating) * 100 : null);

  return (
    <>
      <section className="relative w-full overflow-hidden rounded-4xl bg-gradient-to-br from-[#f8fbff] via-white to-[#e9fbf7] px-6 py-10 text-black shadow-[0px_20px_60px_-30px_rgba(15,23,42,0.35)] sm:px-10 sm:py-12">
        <div className="pointer-events-none absolute -right-20 -top-20 h-72 w-72 rounded-full bg-[#7dd3fc] opacity-30 blur-3xl" />
        <div className="pointer-events-none absolute -left-24 top-24 h-64 w-64 rounded-full bg-[#34d399] opacity-20 blur-3xl" />
        <div className="relative z-1 mx-auto flex w-full max-w-5xl flex-col items-center text-center">
          <p className="text-xs uppercase tracking-[0.3em] text-gray-400">Kaspa</p>
          <h1 className="max-w-3xl text-3xl font-bold sm:text-4xl uppercase tracking-[0.3em]">Addresses</h1>
          <p className="mt-3 max-w-3xl text-sm text-gray-500">
            An address is a unique identifier on the blockchain used to send, receive, and store assets or data. It
            holds balances and interacts with the network securely.
          </p>
          <div className="mt-6 grid w-full grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
            <Card
              title="Number of addresses"
              value={metrics?.numberOfAddresses ? numeral(metrics.numberOfAddresses).format("0,") : "--"}
              subtext="distinct addresses"
              variant="analytics"
            />
            <Card
              title="Top 10 addresses"
              loading={isLoadingSupply}
              value={top10Pct !== null ? `${numeral(top10Pct).format("0.00")}%` : "--"}
              subtext="of circulating supply"
              variant="analytics"
            />
            <Card
              title="Top 100 addresses"
              loading={isLoadingSupply}
              value={top100Pct !== null ? `${numeral(top100Pct).format("0.00")}%` : "--"}
              subtext="of circulating supply"
              variant="analytics"
            />
            <Card
              title="Top 1000 addresses"
              loading={isLoadingSupply}
              value={top1000Pct !== null ? `${numeral(top1000Pct).format("0.00")}%` : "--"}
              subtext="of circulating supply"
              variant="analytics"
            />
          </div>
        </div>
      </section>

      <div className="flex w-full flex-col rounded-4xl bg-white p-4 text-left text-gray-500 sm:p-8">
        <div className="flex items-center justify-end text-xs uppercase tracking-[0.3em] text-gray-400">
          Data computed by vProgs
        </div>
        <PageTable
          className="text-black"
          headers={["Rank", "Address", "Balance", "Percentage"]}
          rows={rows}
          rowClassName={(index) => (index % 2 === 1 ? "bg-gray-25" : "")}
        />
      </div>
      <FooterHelper icon={AccountBalanceWallet}>
        <span className="">
          An address is a unique identifier on the blockchain used to send, receive, and store assets or data. It holds
          balances and interacts with the network securely.
        </span>
      </FooterHelper>
    </>
  );
}
