import Spinner from "./Spinner";
import KasLink from "./KasLink";
import AccountBalanceWallet from "./assets/account_balance_wallet.svg";
import BackToTab from "./assets/back_to_tab.svg";
import Box from "./assets/box.svg";
import Coins from "./assets/coins.svg";
import Dag from "./assets/dag.svg";
import BarChart from "./assets/bar_chart.svg";
import FlashOn from "./assets/flash_on.svg";
import Kaspa from "./assets/kaspa.svg";
import KaspaDifferent from "./assets/kaspadifferent.svg";
import Landslide from "./assets/landslide.svg";
import Rocket from "./assets/rocket_launch.svg";
import Swap from "./assets/swap.svg";
import Time from "./assets/time.svg";
import Trophy from "./assets/trophy.svg";
import VerifiedUser from "./assets/verified_user.svg";
import LastUpdated from "./header/LastUpdated";
import SearchBox from "./header/SearchBox";
import { MarketDataContext } from "./context/MarketDataProvider";
import { useAddressBalance } from "./hooks/useAddressBalance";
import { useAddressBalanceFlowLatest } from "./hooks/useAddressBalanceFlow";
import { useAddressTxCount } from "./hooks/useAddressTxCount";
import { useAddressUtxos } from "./hooks/useAddressUtxos";
import { useBlockdagInfo } from "./hooks/useBlockDagInfo";
import { useBlockReward } from "./hooks/useBlockReward";
import { useCoinSupply } from "./hooks/useCoinSupply";
import { useHalving } from "./hooks/useHalving";
import { useHashrate } from "./hooks/useHashrate";
import { useNetworkCounts } from "./hooks/useNetworkCounts";
import numeral from "numeral";
import { NavLink } from "react-router";
import { useContext, useEffect, useMemo, useState } from "react";

const TOTAL_SUPPLY = 28_700_000_000;
const SAVED_ADDRESS_KEY = "kaspaExplorerSavedAddress";

const formatDifficulty = (value: number) => {
  if (!Number.isFinite(value) || value <= 0) {
    return { value: "0", unit: "" };
  }

  const units = ["", "K", "M", "G", "T", "P", "E"];
  let unitIndex = 0;
  let scaled = value;
  while (scaled >= 1000 && unitIndex < units.length - 1) {
    scaled /= 1000;
    unitIndex += 1;
  }

  return {
    value: numeral(scaled).format("0,0.[00]"),
    unit: units[unitIndex],
  };
};

const Dashboard = () => {
  const [search, setSearch] = useState("");
  const [savedAddress, setSavedAddress] = useState<string | null>(null);
  const marketData = useContext(MarketDataContext);

  const { data: blockDagInfo, isLoading: isLoadingBlockDagInfo } = useBlockdagInfo();
  const { data: coinSupply, isLoading: isLoadingCoinSupply } = useCoinSupply();
  const { data: blockReward, isLoading: isLoadingBlockReward } = useBlockReward();
  const { data: halving, isLoading: isLoadingHalving } = useHalving();
  const { data: hashrate, isLoading: isLoadingHashrate } = useHashrate();
  const { data: networkCounts, isLoading: isLoadingNetworkCounts } = useNetworkCounts();

  const hashrateDisplay = isLoadingHashrate ? { value: "", unit: "" } : formatHashrate(hashrate?.hashrate ?? 0);
  const difficultyDisplay = isLoadingBlockDagInfo
    ? { value: "", unit: "" }
    : formatDifficulty(blockDagInfo?.difficulty ?? 0);
  const totalTransactions = networkCounts?.totalTransactions ?? 0;
  const numberOfAddresses = networkCounts?.numberOfAddresses ?? 0;
  const totalFeesSompi = networkCounts?.totalFeesSompi;
  const totalTransactionsDisplay = totalTransactions ? numeral(totalTransactions).format("0,0") : "--";
  const numberOfAddressesDisplay = numberOfAddresses ? numeral(numberOfAddresses).format("0,0") : "--";
  const totalFeesKas =
    typeof totalFeesSompi === "number" && Number.isFinite(totalFeesSompi) ? totalFeesSompi / 1_0000_0000 : null;
  const totalFeesDisplay = totalFeesKas === null ? "--" : numeral(totalFeesKas).format("0,0");

  useEffect(() => {
    if (typeof window === "undefined") return;
    const loadSavedAddress = () => {
      const value = window.localStorage.getItem(SAVED_ADDRESS_KEY);
      setSavedAddress(value || null);
    };
    loadSavedAddress();
    const handleSavedAddress = () => loadSavedAddress();
    window.addEventListener("kaspa:saved-address", handleSavedAddress as EventListener);
    window.addEventListener("storage", handleSavedAddress);
    return () => {
      window.removeEventListener("kaspa:saved-address", handleSavedAddress as EventListener);
      window.removeEventListener("storage", handleSavedAddress);
    };
  }, []);

  return (
    <>
      <section className="relative overflow-hidden rounded-4xl bg-gradient-to-br from-[#f8fbff] via-white to-[#e9fbf7] px-4 py-16 text-black shadow-[0px_20px_60px_-30px_rgba(15,23,42,0.35)] sm:px-8 sm:py-14 md:py-20 lg:px-24 xl:px-36">
        <div className="pointer-events-none absolute -right-20 -top-20 h-72 w-72 rounded-full bg-[#7dd3fc] opacity-30 blur-3xl" />
        <div className="pointer-events-none absolute -left-24 top-24 h-64 w-64 rounded-full bg-[#34d399] opacity-20 blur-3xl" />
        <div className="relative z-1 grid grid-cols-1 items-center md:grid-cols-[6fr_5fr] md:ps-20">
          <div className="flex w-full flex-col gap-y-3 justify-center">
            <span className="text-3xl lg:text-[54px]">Kaspa Explorer</span>
            <span className="mb-6 text-lg">
              Kaspa is the fastest, open-source, decentralized & fully scalable Layer-1 PoW network in the world.
            </span>
            <SearchBox value={search} onChange={setSearch} className="w-full py-4" />
            {savedAddress && (
              <div className="mt-6 rounded-3xl border border-gray-200 bg-white">
                <SavedAddressCard address={savedAddress} price={Number(marketData?.price ?? 0)} />
              </div>
            )}
          </div>
          <Dag className="w-full h-full md:ps-13 mt-2 md:mt-0" />
        </div>
      </section>
      <div className="flex justify-center py-6 text-center text-xs uppercase tracking-wide text-gray-400 sm:py-8 md:py-10">
        <LastUpdated />
      </div>
      <div className="flex w-full flex-col rounded-4xl bg-gray-50 px-4 py-12 text-white sm:px-8 sm:py-12 md:px-20 md:py-20 lg:px-24 lg:py-24 xl:px-36 xl:py-26">
        <span className="mb-7 text-black text-3xl md:text-4xl lg:text-5xl">Kaspa by the numbers</span>
        <div className="grid grid-cols-1 gap-x-4 gap-y-4 sm:grid-cols-2 lg:grid-cols-4">
          <DashboardBox
            description="Network hashrate"
            value={hashrateDisplay.value}
            unit={hashrateDisplay.unit}
            icon={<FlashOn className="w-5" />}
            loading={isLoadingHashrate}
          />
          <DashboardBox
            description="Network difficulty"
            value={difficultyDisplay.value}
            unit={difficultyDisplay.unit}
            icon={<BarChart className="w-5" />}
            loading={isLoadingBlockDagInfo}
          />
          <DashboardBox
            description="Total blocks"
            value={numeral(blockDagInfo?.virtualDaaScore || 0).format("0,0")}
            icon={<Box className="w-5" />}
            loading={isLoadingBlockDagInfo}
          />
          <DashboardBox
            description="Total transactions"
            value={totalTransactionsDisplay}
            icon={<Swap className="w-5" />}
            loading={isLoadingNetworkCounts}
          />
          <DashboardBox
            description="Total fees"
            value={totalFeesDisplay}
            unit="KAS"
            icon={<Coins className="w-5" />}
            loading={isLoadingNetworkCounts}
          />
          <DashboardBox
            description="Total supply"
            value={numeral((coinSupply?.circulatingSupply || 0) / 1_0000_0000).format("0,0")}
            unit="KAS"
            icon={<Coins className="w-5" />}
            loading={isLoadingCoinSupply}
          />
          <DashboardBox
            description="Number of addresses"
            value={numberOfAddressesDisplay}
            icon={<AccountBalanceWallet className="w-5" />}
            loading={isLoadingNetworkCounts}
          />
          <DashboardBox
            description="Mined"
            value={((coinSupply?.circulatingSupply || 0) / TOTAL_SUPPLY / 1000000).toFixed(2)}
            unit="%"
            icon={<Landslide className="w-5" />}
            loading={isLoadingCoinSupply}
          />
          <DashboardBox description="Average block time" value={"0.1"} unit="s" icon={<Time className="w-5" />} />
          <DashboardBox
            description="Block reward"
            value={(blockReward?.blockreward || 0).toFixed(3)}
            unit="KAS"
            icon={<Trophy className="w-5" />}
            loading={isLoadingBlockReward}
          />
          <DashboardBox
            description="Reward reduction"
            value={halving?.nextHalvingDate || ""}
            icon={<Swap className="w-5" />}
            loading={isLoadingHalving}
          />
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-6 gap-y-4 gap-x-4 px-4 pt-4 pb-10 text-black sm:px-8 sm:pt-6 sm:pb-12 md:px-20 md:pt-8 md:pb-20 lg:flex-row lg:px-24 lg:pt-12 lg:pb-24 xl:px-36 xl:pt-14 xl:pb-38">
        <div className="col-span-1 md:col-span-3">
          <DashboardInfoBox
            description="A digital ledger enabling parallel blocks and instant transaction confirmation –
          built on a robust proof-of-work engine with rapid single-second block intervals."
            title="The world’s first BlockDAG"
            icon={<Rocket className="w-5" />}
          />
        </div>
        <div className="col-span-1 md:col-span-3">
          <DashboardInfoBox
            description="Kaspa enables near-instant transaction confirmations, ensuring seamless and efficient user experiences for payments and transfers."
            title="Instant Transactions"
            icon={<Rocket className="w-5" />}
          />
        </div>
        <div className="col-span-1 md:col-span-2">
          <DashboardInfoBox
            description="Designed with scalability in mind, Kaspa handles high transaction volumes without compromising speed or decentralization."
            title="Scalable Network"
            icon={<BackToTab className="w-5" />}
          />
        </div>
        <div className="col-span-1 md:col-span-2">
          <DashboardInfoBox
            description="Kaspa uses innovative technology to minimize energy consumption, making it a greener choice in blockchain networks."
            title="Energy Efficiency"
            icon={<FlashOn className="w-5" />}
          />
        </div>
        <div className="col-span-1 md:col-span-2">
          <DashboardInfoBox
            description="With its robust and decentralized infrastructure, Kaspa ensures secure transactions without reliance on central authorities."
            title="Decentralized Security"
            icon={<VerifiedUser className="w-5" />}
          />
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 w-full gap-y-3 rounded-4xl bg-white px-4 py-12 sm:px-8 sm:py-12 md:px-20 md:py-20 lg:px-24 lg:py-24 xl:px-36 xl:py-38">
        <div className="flex flex-col">
          <div className="text-5xl">Kaspa is built differently.</div>
          <div className="text-md text-gray-500 mt-2 mb-2">
            Kaspa is a community project – completely open source with no central governance – following in the ethos of
            coins like Bitcoin.
          </div>
        </div>
        <div className="flex flex-row items-center justify-center md:justify-end">
          <KaspaDifferent className="" />
        </div>
      </div>
    </>
  );
};

const SavedAddressCard = ({ address, price }: { address: string; price: number }) => {
  const { data, isLoading: isLoadingBalance } = useAddressBalance(address);
  const { data: txCount, isLoading: isLoadingTxCount } = useAddressTxCount(address);
  const { data: utxos, isLoading: isLoadingUtxos } = useAddressUtxos(address);
  const absoluteLimit = 10000;
  const hasTxCount = typeof txCount?.total === "number";
  const latestLimitHint = useMemo(() => {
    if (!hasTxCount) return null;
    const acceptedCount = typeof txCount?.accepted === "number" ? txCount.accepted : null;
    return Math.max(1, Math.min(absoluteLimit, acceptedCount ?? txCount.total));
  }, [hasTxCount, txCount, absoluteLimit]);
  const { data: balanceFlowLatest, refetch: refetchBalanceFlowLatest } = useAddressBalanceFlowLatest(
    address,
    latestLimitHint ?? absoluteLimit,
    600000,
    hasTxCount && latestLimitHint !== null,
  );
  const LoadingSpinner = () => <Spinner className="h-5 w-5" />;
  const balance = numeral((data?.balance || 0) / 1_0000_0000).format("0,0.00[000000]");
  const usdValue = numeral(((data?.balance || 0) / 1_0000_0000) * (price || 0)).format("$0,0.00");
  const balanceFlowPoints = useMemo(() => {
    const points = balanceFlowLatest?.points || [];
    return points
      .filter((point) => Number.isFinite(point.balance) && Number.isFinite(point.timestamp))
      .sort((a, b) => a.timestamp - b.timestamp);
  }, [balanceFlowLatest]);
  const changePercent = useMemo(() => {
    const points = balanceFlowPoints || [];
    if (points.length < 2) return null;
    const sorted = [...points].sort((a, b) => a.timestamp - b.timestamp);
    const latest = sorted[sorted.length - 1];
    const cutoff = latest.timestamp - 24 * 60 * 60 * 1000;
    let past = sorted[0];
    for (const point of sorted) {
      if (point.timestamp <= cutoff) past = point;
      else break;
    }
    if (!past || past.balance <= 0) return null;
    const percent = ((latest.balance - past.balance) / past.balance) * 100;
    return Number.isFinite(percent) ? percent : null;
  }, [balanceFlowPoints]);
  const changeKas = useMemo(() => {
    const points = balanceFlowPoints || [];
    if (points.length < 2) return null;
    const sorted = [...points].sort((a, b) => a.timestamp - b.timestamp);
    const latest = sorted[sorted.length - 1];
    const cutoff = latest.timestamp - 24 * 60 * 60 * 1000;
    let past = sorted[0];
    for (const point of sorted) {
      if (point.timestamp <= cutoff) past = point;
      else break;
    }
    if (!past) return null;
    const delta = (latest.balance - past.balance) / 1_0000_0000;
    return Number.isFinite(delta) ? delta : null;
  }, [balanceFlowPoints]);
  const changeLabel =
    changePercent === null ? "--" : `${changePercent >= 0 ? "+" : ""}${numeral(changePercent).format("0.00")}`;
  const changeKasLabel =
    changeKas === null ? "--" : `${changeKas >= 0 ? "+" : ""}${numeral(changeKas).format("0,0.00[0000]")} KAS`;
  const changeClass = changePercent === null ? "text-gray-400" : changePercent >= 0 ? "text-success" : "text-error";

  useEffect(() => {
    if (!hasTxCount || latestLimitHint === null) return;
    const intervalId = window.setInterval(() => {
      refetchBalanceFlowLatest();
    }, 600000);
    return () => window.clearInterval(intervalId);
  }, [hasTxCount, latestLimitHint, refetchBalanceFlowLatest]);

  return (
    <div className="flex w-full flex-col rounded-4xl bg-white p-4 text-left text-black sm:p-8">
      <div className="flex flex-row items-center justify-between text-2xl sm:col-span-2">
        <div className="flex items-center">
          <AccountBalanceWallet className="mr-2 h-8 w-8" />
          <span>My wallet address</span>
        </div>
        <NavLink
          to={`/addresses/${address}`}
          className="rounded-full border px-4 py-2 text-sm font-medium text-black transition hover:bg-gray-50"
          style={{ borderColor: "#70C7BA", backgroundColor: "transparent" }}
        >
          Open address details
        </NavLink>
      </div>

      <span className="mt-4 mb-0">Balance</span>

      {!isLoadingBalance ? (
        <div className="flex flex-wrap items-center gap-3">
          <span className="flex flex-row items-center text-[32px]">
            {balance.split(".")[0]}.<span className="self-end pb-[0.4rem] text-2xl">{balance.split(".")[1]}</span>
            <Kaspa className="fill-primary ml-1 h-8 w-8" />
          </span>
          <span
            className={`flex h-6 min-w-fit items-center justify-around gap-x-1 rounded-4xl border border-gray-100 bg-white p-1 pr-2 text-sm ${changeClass}`}
          >
            <span className="text-gray-700">{changeKasLabel}</span>
            <span className="ms-1">
              {changeLabel}
              <span className="ms-[1px]">%</span>
            </span>
          </span>
        </div>
      ) : (
        <LoadingSpinner />
      )}
      {!isLoadingBalance ? <span className="ml-1 text-gray-500">{usdValue}</span> : <LoadingSpinner />}
      <div className={`my-4 h-[1px] bg-gray-100 sm:col-span-2`} />

      <div className="grid grid-cols-1 gap-x-14 gap-y-2 sm:grid-cols-[auto_1fr]">
        <div className="flex flex-row items-start fill-gray-500 text-gray-500 sm:col-start-1">
          <span>Address</span>
        </div>
        <div className="break-all text-wrap">
          <KasLink linkType="address" copy qr to={address} />
        </div>
        <div className="flex flex-row items-start fill-gray-500 text-gray-500 sm:col-start-1">
          <span>Transactions</span>
        </div>
        <div className="break-all text-wrap">
          {!isLoadingTxCount ? numeral(txCount?.total || 0).format("0,") : <LoadingSpinner />}
        </div>
        <div className="flex flex-row items-start fill-gray-500 text-gray-500 sm:col-start-1">
          <span>UTXOs</span>
        </div>
        <div className="break-all text-wrap">{!isLoadingUtxos ? numeral(utxos?.length || 0).format("0,") : <LoadingSpinner />}</div>
      </div>
    </div>
  );
};

interface DashboardBoxProps {
  icon: React.ReactNode;
  description: string;
  value: string | number;
  unit?: string;
  loading?: boolean;
}

const DashboardBox = (props: DashboardBoxProps) => {
  return (
    <div className="flex flex-col gap-y-2 rounded-2xl border border-gray-200 px-6 py-4">
      <div className="flex flex-row items-center overflow-hidden text-lg">
        <div className="fill-primary mr-1 w-5">{props.icon}</div>
        <span className="text-gray-500">{props.description}</span>
      </div>
      <span className="md:text-lg xl:text-xl text-black">
        {!props.loading ? (
          props.value
        ) : (
          <span>
            <Spinner className="mr-2 inline h-5 w-5" />
          </span>
        )}
        {props.unit ? <span className="text-gray-500 md:text-md xl:text-lg"> {props.unit}</span> : ""}
      </span>
    </div>
  );
};

export default Dashboard;

interface InfoBoxProps {
  icon: React.ReactNode;
  title: string;
  description: string;
}

const DashboardInfoBox = (props: InfoBoxProps) => {
  return (
    <div className="flex flex-col h-full gap-y-2 bg-white p-8 rounded-2xl">
      <>{props.icon}</>
      <span className="text-xl">{props.title}</span>
      <span className="text-gray-500">{props.description}</span>
    </div>
  );
};

const formatHashrate = (hashrateTh: number) => {
  if (!Number.isFinite(hashrateTh) || hashrateTh <= 0) {
    return { value: "0", unit: "TH/s" };
  }
  const units = ["TH/s", "PH/s", "EH/s", "ZH/s"];
  let unitIndex = 0;
  let value = hashrateTh;
  while (value >= 1000 && unitIndex < units.length - 1) {
    value /= 1000;
    unitIndex += 1;
  }
  return { value: numeral(value).format("0,0.[00]"), unit: units[unitIndex] };
};
