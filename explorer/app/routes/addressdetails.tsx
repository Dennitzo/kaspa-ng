import { Accepted, NotAccepted } from "../Accepted";
import Coinbase from "../Coinbase";
import IconMessageBox from "../IconMessageBox";
import KasLink from "../KasLink";
import PageSelector from "../PageSelector";
import PageTable from "../PageTable";
import Spinner from "../Spinner";
import Tooltip, { TooltipDisplayMode } from "../Tooltip";
import AccountBalanceWallet from "../assets/account_balance_wallet.svg";
import ArrowRight from "../assets/arrow-right.svg";
import Chart from "../assets/chart.svg";
import Info from "../assets/info.svg";
import Kaspa from "../assets/kaspa.svg";
import { VPROGS_BASE } from "../api/urls";
import { MarketDataContext } from "../context/MarketDataProvider";
import { useAddressBalance } from "../hooks/useAddressBalance";
import { useAddressBalanceFlow, useAddressBalanceFlowLatest } from "../hooks/useAddressBalanceFlow";
import { useAddressTxCount } from "../hooks/useAddressTxCount";
import { useAddressUtxos } from "../hooks/useAddressUtxos";
import { useTransactions } from "../hooks/useTransactions";
import { useTransactionsSearch } from "../hooks/useTransactionsSearch";
import FooterHelper from "../layout/FooterHelper";
import { isValidKaspaAddressSyntax } from "../utils/kaspa";
import type { Route } from "./+types/addressdetails";
import dayjs from "dayjs";
import localeData from "dayjs/plugin/localeData";
import localizedFormat from "dayjs/plugin/localizedFormat";
import relativeTime from "dayjs/plugin/relativeTime";
import numeral from "numeral";
import React, { useContext, useEffect, useMemo, useRef, useState } from "react";
import { NavLink, useLocation } from "react-router";

const SAVED_ADDRESS_KEY = "kaspaExplorerSavedAddress";

dayjs().locale("en");
dayjs.extend(relativeTime);
dayjs.extend(localeData);
dayjs.extend(localizedFormat);

export async function loader({ params }: Route.LoaderArgs) {
  const address = params.address;
  if (!address) {
    throw new Response("Missing Kaspa address.", { status: 400 });
  }
  if (isValidKaspaAddressSyntax(address)) {
    return { address };
  }
  const withPrefix = `kaspa:${address}`;
  if (isValidKaspaAddressSyntax(withPrefix)) {
    return { address: withPrefix };
  }
  throw new Response(`Kaspa address ${address} doesn't follow the kaspa address schema.`, { status: 400 });
}

export function meta({ params }: Route.LoaderArgs) {
  return [
    { title: `Kaspa Address ${params.address} | Kaspa Explorer` },
    {
      name: "description",
      content: "Check Kaspa address details. View transaction history, balance, and associated blocks.",
    },
    { name: "keywords", content: "Kaspa address, transactions, wallet balance, blockchain address lookup" },
  ];
}

export default function Addressdetails({ loaderData }: Route.ComponentProps) {
  const location = useLocation();
  const { data, isLoading: isLoadingAddressBalance, isFetching: isFetchingAddressBalance } =
    useAddressBalance(loaderData.address);
  const { data: utxoData, isLoading: isLoadingUtxoData } = useAddressUtxos(loaderData.address);
  const { data: txCount, isLoading: isLoadingTxCount } = useAddressTxCount(loaderData.address);
  const marketData = useContext(MarketDataContext);
  const [beforeAfter, setBeforeAfter] = useState<number[]>([0, 0]);
  const [currentPage, setCurrentPage] = useState<number>(1);
  const [lastUpdated, setLastUpdated] = useState<string>("--");
  const [expand, setExpand] = useState<string[]>([]);
  const [shouldLoadBalanceFlow, setShouldLoadBalanceFlow] = useState(false);
  const absoluteLimit = 10000;
  const hasTxCount = typeof txCount?.total === "number";
  const latestLimitHint = useMemo(() => {
    if (!hasTxCount) return null;
    const acceptedCount = typeof txCount?.accepted === "number" ? txCount.accepted : null;
    return Math.max(1, Math.min(absoluteLimit, acceptedCount ?? txCount.total));
  }, [hasTxCount, txCount, absoluteLimit]);
  const [flowLimit, setFlowLimit] = useState<number | null>(null);
  const {
    data: balanceFlowData,
    refetch: refetchBalanceFlow,
    isLoading: isLoadingBalanceFlow,
    isFetching: isFetchingBalanceFlow,
  } = useAddressBalanceFlow(
    loaderData.address,
    flowLimit ?? absoluteLimit,
    shouldLoadBalanceFlow && flowLimit !== null && hasTxCount ? 30000 : false,
    shouldLoadBalanceFlow && flowLimit !== null && hasTxCount,
  );
  const hasLoadedBalanceFlowRef = useRef(false);
  const {
    data: balanceFlowLatest,
    refetch: refetchBalanceFlowLatest,
    isFetching: isFetchingBalanceFlowLatest,
  } = useAddressBalanceFlowLatest(
    loaderData.address,
    latestLimitHint ?? absoluteLimit,
    600000,
    hasTxCount && latestLimitHint !== null,
  );
  const latestLimit = useMemo(() => {
    if (!hasTxCount) return null;
    const acceptedHint = balanceFlowLatest?.points?.length;
    const totalCap = latestLimitHint ?? absoluteLimit;
    if (typeof acceptedHint === "number" && acceptedHint > 0) {
      return Math.max(1, Math.min(totalCap, acceptedHint));
    }
    return totalCap;
  }, [hasTxCount, absoluteLimit, balanceFlowLatest, latestLimitHint]);
  const vprogsBaseUrl = VPROGS_BASE;
  const [isSavedAddress, setIsSavedAddress] = useState(false);
  const vprogsBase = useMemo(() => (vprogsBaseUrl ? vprogsBaseUrl.replace(/\/$/, "") : ""), [vprogsBaseUrl]);

  useEffect(() => {
    setBeforeAfter([0, 0]); // Reset beforeAfter state
    setCurrentPage(1); // Reset currentPage state
    hasLoadedBalanceFlowRef.current = false;
    setShouldLoadBalanceFlow(false);
    setFlowLimit(null);
    if (typeof window !== "undefined") {
      const saved = window.localStorage.getItem(SAVED_ADDRESS_KEY);
      setIsSavedAddress(saved === loaderData.address);
      if (saved === loaderData.address) {
        void upsertSavedAddressLabel();
      }
    }
  }, [loaderData.address, vprogsBase]);

  const pageSize = 25;

  // fetch transactions with resolve_previous_outpoints set to "light"
  const { data: txData } = useTransactions(
    loaderData.address,
    pageSize,
    currentPage === 1 ? 0 : beforeAfter[0],
    currentPage === 1 ? 0 : beforeAfter[1],
    "",
    "light",
  );

  const pageChange = (page: number) => {
    // FIRST = 0,
    // LAST = 3,
    // PREVIOUS = 2,
    // NEXT = 1,
    if (page === 0) {
      setBeforeAfter([0, 0]);
      setCurrentPage(1);
    } else if (page === 1) {
      setBeforeAfter([txData?.nextBefore ?? 0, 0]);
      setCurrentPage((currentPage) => currentPage + 1);
    } else if (page === 2) {
      setBeforeAfter([0, txData?.nextAfter ?? 0]);
      setCurrentPage((currentPage) => currentPage - 1);
    } else if (page === 3) {
      setBeforeAfter([0, 1]);
      setCurrentPage(Math.ceil(txCount!.total / pageSize));
    }
  };

  const transactions = txData?.transactions || [];

  if (!loaderData.address) return;

  const isTabActive = (tab: string) => {
    const params = new URLSearchParams(location.search);
    if (tab === "transactions" && params.get("tab") === null) return true;
    return params.get("tab") === tab;
  };

  const txFilter = new URLSearchParams(location.search).get("tx") || "accepted";
  const filteredTransactions =
    isTabActive("transactions") && txFilter === "accepted"
      ? transactions.filter((transaction) => transaction.is_accepted)
      : transactions;
  const utxoTxIds = useMemo(() => {
    const ids = new Set<string>();
    (utxoData?.slice(0, 50) || []).forEach((utxo) => ids.add(utxo.outpoint.transactionId));
    return Array.from(ids);
  }, [utxoData]);

  const { data: utxoTxData } = useTransactionsSearch(
    utxoTxIds,
    "block_time,transaction_id",
    "no",
    isTabActive("utxos") && utxoTxIds.length > 0,
    60000,
  );
  const utxoTxTimeById = useMemo(
    () => new Map((utxoTxData || []).map((transaction) => [transaction.transaction_id, transaction.block_time])),
    [utxoTxData],
  );

  const balanceFlowPoints = useMemo(() => {
    const points = balanceFlowData?.points || balanceFlowLatest?.points || [];
    return points
      .filter((point) => Number.isFinite(point.balance) && Number.isFinite(point.timestamp))
      .sort((a, b) => a.timestamp - b.timestamp);
  }, [balanceFlowData, balanceFlowLatest]);

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

  const upsertSavedAddressLabel = async () => {
    if (!vprogsBase) return;
    try {
      await fetch(`${vprogsBase}/api/address-labels`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ address: loaderData.address, label: "My wallet" }),
      });
    } catch {
      // ignore label save errors
    }
  };

  const deleteSavedAddressLabel = async (address: string) => {
    if (!vprogsBase || !address) return;
    try {
      await fetch(`${vprogsBase}/api/address-labels/${encodeURIComponent(address)}`, {
        method: "DELETE",
      });
    } catch {
      // ignore label delete errors
    }
  };

  const toggleSavedAddress = () => {
    if (typeof window === "undefined") return;
    if (isSavedAddress) {
      const previous = window.localStorage.getItem(SAVED_ADDRESS_KEY);
      window.localStorage.removeItem(SAVED_ADDRESS_KEY);
      setIsSavedAddress(false);
      window.dispatchEvent(new CustomEvent("kaspa:saved-address", { detail: null }));
      if (previous) {
        void deleteSavedAddressLabel(previous);
      }
    } else {
      const previous = window.localStorage.getItem(SAVED_ADDRESS_KEY);
      window.localStorage.setItem(SAVED_ADDRESS_KEY, loaderData.address);
      setIsSavedAddress(true);
      window.dispatchEvent(new CustomEvent("kaspa:saved-address", { detail: loaderData.address }));
      if (previous && previous !== loaderData.address) {
        void deleteSavedAddressLabel(previous);
      }
      void upsertSavedAddressLabel();
    }
  };


  const balanceFlowRange = useMemo(() => {
    if (!balanceFlowPoints.length) {
      return { min: 0, max: 0 };
    }
    let min = balanceFlowPoints[0].balance;
    let max = balanceFlowPoints[0].balance;
    balanceFlowPoints.forEach((point) => {
      if (point.balance < min) min = point.balance;
      if (point.balance > max) max = point.balance;
    });
    min = Math.max(0, min);
    if (min === max) {
      min -= 1;
      max += 1;
    }
    return { min, max };
  }, [balanceFlowPoints]);

  const balanceFlowTicks = useMemo(() => {
    if (!balanceFlowPoints.length) return [];
    const max = balanceFlowRange.max;
    const min = balanceFlowRange.min;
    const mid = min + (max - min) / 2;
    const ticks = [
      { ratio: 1, value: max },
      { ratio: 0.5, value: mid },
      { ratio: 0, value: min },
    ];
    return ticks.filter((tick) => tick.value >= 0);
  }, [balanceFlowPoints, balanceFlowRange]);

  const balanceFlowTimeSpan = useMemo(() => {
    const timestamps = balanceFlowPoints
      .map((point) => point.timestamp)
      .filter((timestamp): timestamp is number => !!timestamp);
    if (timestamps.length === 0) {
      return { minTime: 0, maxTime: 0, spanMs: 0 };
    }
    const minTime = Math.min(...timestamps);
    const maxTime = Math.max(...timestamps);
    return { minTime, maxTime, spanMs: Math.max(0, maxTime - minTime) };
  }, [balanceFlowPoints]);

  const balanceFlowTickLabels = useMemo(() => {
    if (balanceFlowPoints.length === 0) return [];
    const format = "MMM D, YYYY h:mm:ss A";
    const lastIndex = balanceFlowPoints.length - 1;
    const midIndex = Math.floor(lastIndex / 2);
    const leftTime = balanceFlowPoints[0]?.timestamp;
    const midTime = balanceFlowPoints[midIndex]?.timestamp;
    const rightTime = balanceFlowPoints[lastIndex]?.timestamp;
    return [
      leftTime ? dayjs(leftTime).format(format) : "—",
      midTime ? dayjs(midTime).format(format) : "—",
      rightTime ? dayjs(rightTime).format(format) : "—",
    ];
  }, [balanceFlowPoints, balanceFlowTimeSpan]);

  const isBalanceFlowLoading = isLoadingBalanceFlow || isFetchingBalanceFlow;
  const hasBalanceFlowPoints = balanceFlowPoints.length > 0;
  const isCachedBalanceFlow = balanceFlowData?.cached === true || balanceFlowLatest?.cached === true;
  const showBalanceFlowUpdating = isBalanceFlowLoading && !isCachedBalanceFlow;
  const showBalanceFlowWaiting =
    flowLimit !== null && (isBalanceFlowLoading || (!hasBalanceFlowPoints && !isCachedBalanceFlow));


  const balance = numeral((data?.balance || 0) / 1_0000_0000).format("0,0.00[000000]");
  const LoadingSpinner = () => <Spinner className="h-5 w-5" />;

  useEffect(() => {
    if (!isFetchingAddressBalance && !isLoadingAddressBalance) {
      setLastUpdated(new Date().toLocaleString());
    }
  }, [isFetchingAddressBalance, isLoadingAddressBalance, data]);

  useEffect(() => {
    if (!isFetchingBalanceFlowLatest && balanceFlowLatest) {
      setLastUpdated(new Date().toLocaleString());
    }
  }, [isFetchingBalanceFlowLatest, balanceFlowLatest]);

  useEffect(() => {
    if (!hasTxCount || latestLimitHint === null) return;
    const intervalId = window.setInterval(() => {
      refetchBalanceFlowLatest();
    }, 600000);
    return () => window.clearInterval(intervalId);
  }, [hasTxCount, latestLimitHint, refetchBalanceFlowLatest]);

  useEffect(() => {
    if (!shouldLoadBalanceFlow) return;
    if (!balanceFlowData?.pending) return;
    const timer = window.setTimeout(() => {
      refetchBalanceFlow();
    }, 15000);
    return () => window.clearTimeout(timer);
  }, [shouldLoadBalanceFlow, balanceFlowData, refetchBalanceFlow]);

  useEffect(() => {
    if (!txCount?.total) return;
    if (hasLoadedBalanceFlowRef.current) return;
    const desiredLimit = Math.max(1, Math.min(absoluteLimit, txCount.total));
    const latestLimitValue = balanceFlowLatest?.requestLimit;
    if (balanceFlowLatest?.points && balanceFlowLatest.points.length > 1) {
      if (latestLimitValue && latestLimitValue < desiredLimit) {
        setFlowLimit(desiredLimit);
        setShouldLoadBalanceFlow(true);
      } else {
        setFlowLimit(latestLimitValue || desiredLimit);
      }
      hasLoadedBalanceFlowRef.current = true;
      return;
    }
    setFlowLimit(desiredLimit);
    setShouldLoadBalanceFlow(true);
    hasLoadedBalanceFlowRef.current = true;
  }, [txCount, balanceFlowLatest, absoluteLimit]);

  return (
    <>
      <div className="relative flex w-full flex-col rounded-4xl bg-white p-4 text-left text-black sm:p-8">
        <div className="flex flex-row items-center justify-between text-2xl sm:col-span-2">
          <div className="flex items-center">
            <AccountBalanceWallet className="mr-2 h-8 w-8" />
            <span>Address details</span>
          </div>
          <button
            type="button"
            onClick={toggleSavedAddress}
            className="rounded-full border px-4 py-2 text-sm font-medium transition hover:bg-gray-50"
            style={{ borderColor: "#70C7BA", backgroundColor: "transparent", color: "#70C7BA" }}
          >
            {isSavedAddress ? "Delete as my wallet address" : "Save as my wallet address"}
          </button>
        </div>

        <span className="mt-4 mb-0">Balance</span>

        {!isLoadingAddressBalance ? (
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
        {!isLoadingAddressBalance ? (
          <span className="ml-1 text-gray-500">
            {numeral(((data?.balance || 0) / 1_0000_0000) * (marketData?.price || 0)).format("$0,0.00")}
          </span>
        ) : (
          <LoadingSpinner />
        )}
        {/*horizontal rule*/}
        <div className={`my-4 h-[1px] bg-gray-100 sm:col-span-2`} />

        <div className="grid grid-cols-1 gap-x-14 gap-y-2 sm:grid-cols-[auto_1fr]">
          <FieldName name="Address" infoText="A unique Kaspa address used to send and receive funds." />
          <FieldValue value={<KasLink linkType="address" copy qr to={loaderData.address} />} />
          <FieldName name="Transactions" infoText="Total number of transactions involving this address." />
          <FieldValue value={!isLoadingTxCount ? numeral(txCount!.total).format("0,") : <LoadingSpinner />} />
          <FieldName name="UTXOs" infoText="Unspent, available outputs available at this address." />
          <FieldValue value={!isLoadingUtxoData ? numeral(utxoData!.length).format("0,") : <LoadingSpinner />} />
        </div>
      </div>
      <div className="w-full rounded-4xl bg-white p-4 text-left text-black sm:p-8">
        <div className="flex items-center justify-between">
          <div className="flex flex-row items-center text-2xl">
            <Chart className="mr-2 h-8 w-8" />
            <span>Balance over time</span>
          </div>
          <span className="flex items-center gap-2 text-xs uppercase tracking-[0.3em] text-gray-400">
            {showBalanceFlowUpdating && <LoadingSpinner />}
            <span>Data computed by vProgs</span>
          </span>
        </div>
        <div className="mt-2 flex flex-wrap items-center gap-2 text-sm text-gray-500">
          <span>
            {(() => {
              const acceptedCount = txCount?.accepted ?? txCount?.total ?? 0;
              const limitValue = flowLimit ?? absoluteLimit;
              const waitingValue = Math.min(acceptedCount, limitValue);
              const displayValue = Math.max(
                0,
                showBalanceFlowWaiting
                  ? waitingValue
                  : balanceFlowPoints.length > 0
                    ? balanceFlowPoints.length
                    : limitValue,
              );
              return (
                <>
                  Latest {numeral(displayValue).format("0,0")} accepted transactions, computed via vProgs flow.
                </>
              );
            })()}
          </span>
        </div>
        {hasBalanceFlowPoints ? (
          <div className="mt-4">
            <div className="flex gap-3">
              <div className="relative flex h-44 w-16 flex-col justify-between pr-2 text-xs text-gray-400">
                <div className="absolute right-0 top-0 h-full w-px bg-gray-200" />
                {balanceFlowTicks.map((tick, index) => (
                  <div key={`${tick.value}-${index}`} className="flex items-center justify-end">
                    <span>{`${numeral(tick.value / 1_0000_0000).format("0,0")} KAS`}</span>
                  </div>
                ))}
              </div>
              <div className="relative h-44 flex-1 rounded-2xl border border-gray-100 bg-gray-25 px-3">
                <div className="absolute bottom-3 left-0 right-0 h-px bg-gray-200" />
                <svg className="absolute inset-0 h-full w-full" viewBox="0 0 1000 180" preserveAspectRatio="none">
                  {(() => {
                    const count = balanceFlowPoints.length;
                    if (count === 0) return null;
                    const coords = balanceFlowPoints.map((point, index) => {
                      const x = count === 1 ? 0 : (index / (count - 1)) * 1000;
                      const yRatio =
                        (point.balance - balanceFlowRange.min) / (balanceFlowRange.max - balanceFlowRange.min || 1);
                      const clampedRatio = Math.min(1, Math.max(0, yRatio));
                      const y = 170 - clampedRatio * 150;
                      return { x, y };
                    });
                    const stepPoints: string[] = [];
                    coords.forEach((coord, index) => {
                      if (index === 0) {
                        stepPoints.push(`${coord.x},${coord.y}`);
                        return;
                      }
                      const prev = coords[index - 1];
                      stepPoints.push(`${coord.x},${prev.y}`);
                      stepPoints.push(`${coord.x},${coord.y}`);
                    });
                    const first = coords[0];
                    const last = coords[coords.length - 1];
                    const areaPath = `M ${stepPoints[0]} L ${stepPoints.join(" ")} L ${last.x},170 L ${first.x},170 Z`;
                    return (
                      <>
                        <path d={areaPath} fill="rgba(112, 199, 186, 0.18)" stroke="none" />
                        <polyline
                          fill="none"
                          stroke="#70C7BA"
                          strokeWidth="2"
                          points={stepPoints.join(" ")}
                        />
                      </>
                    );
                  })()}
                </svg>
                {balanceFlowPoints.map((point, index) => {
                  const xPercent = (index / (balanceFlowPoints.length - 1)) * 100;
                  const yRatio =
                    (point.balance - balanceFlowRange.min) / (balanceFlowRange.max - balanceFlowRange.min || 1);
                  const clampedRatio = Math.min(1, Math.max(0, yRatio));
                  const yPercent = (1 - clampedRatio) * 100;
                  const timestamp = point.timestamp ? dayjs(point.timestamp).format("MMM D, YYYY HH:mm:ss") : "—";
                  const balanceKas = numeral(point.balance / 1_0000_0000).format("0,0.00[000000]");
                  return (
                    <Tooltip
                      key={`${point.timestamp}-${index}`}
                      message={`Balance:\u00A0${balanceKas}\u00A0KAS\nTimestamp: ${timestamp}`}
                      display={TooltipDisplayMode.Hover}
                      multiLine
                      className="absolute"
                      triggerClassName="absolute"
                      style={{
                        left: `${xPercent}%`,
                        top: `${yPercent}%`,
                        transform: "translate(-50%, -50%)",
                      }}
                    >
                      <div className="h-6 w-6 rounded-full bg-transparent" />
                    </Tooltip>
                  );
                })}
              </div>
            </div>
            <div className="mt-2 flex w-full justify-between ps-16 text-xs text-gray-400">
              {balanceFlowTickLabels.map((label, index) => (
                <div key={`${label}-${index}`} className="flex flex-col items-center">
                  <div className="h-2 w-px bg-gray-200" />
                  <span className="mt-1">{label}</span>
                </div>
              ))}
            </div>
          </div>
        ) : (
          <div className="mt-4 rounded-2xl border border-dashed border-gray-100 bg-gray-25 p-6 text-center text-sm text-gray-500">
            {flowLimit === null ? (
              <div className="flex flex-col items-center gap-3">
                <LoadingSpinner />
                <span>Waiting for transaction count…</span>
              </div>
            ) : (isBalanceFlowLoading && !isCachedBalanceFlow) || !hasBalanceFlowPoints ? (
              <div className="flex flex-col items-center gap-3">
                <LoadingSpinner />
                <span>Waiting for vProgs computation…</span>
              </div>
            ) : (
              "No balance history available yet."
            )}
          </div>
        )}
      </div>
      <div className="my-2 text-center text-xs uppercase tracking-wide text-gray-400">
        Last updated: <span className="text-gray-500">{lastUpdated}</span>
      </div>
      <div className="flex w-full flex-col gap-x-18 gap-y-6 rounded-4xl bg-white p-4 text-left text-black sm:p-8">
        <div className="flex w-full flex-col items-start gap-3 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex w-auto flex-row items-center justify-around gap-x-1 rounded-full bg-gray-50 p-1 px-1">
            <NavLink
              to={`/addresses/${loaderData.address}?tab=transactions`}
              preventScrollReset={true}
              className={() =>
                `rounded-full px-4 py-1.5 hover:cursor-pointer hover:bg-white ${isTabActive("transactions") ? "bg-white" : ""}`
              }
            >
              Transactions
            </NavLink>
            <NavLink
              to={`/addresses/${loaderData.address}?tab=utxos`}
              preventScrollReset={true}
              className={() =>
                `rounded-full px-4 py-1.5 hover:cursor-pointer hover:bg-white ${isTabActive("utxos") ? "bg-white" : ""}`
              }
            >
              UTXOs
            </NavLink>
          </div>
          {isTabActive("transactions") && (
            <div className="flex w-auto flex-row items-center justify-around gap-x-1 rounded-full bg-gray-50 p-1 px-1">
              <NavLink
                to={`/addresses/${loaderData.address}?tab=transactions&tx=all`}
                preventScrollReset={true}
                className={() =>
                  `rounded-full px-4 py-1.5 hover:cursor-pointer hover:bg-white ${txFilter === "all" ? "bg-white" : ""}`
                }
              >
                All
              </NavLink>
              <NavLink
                to={`/addresses/${loaderData.address}?tab=transactions&tx=accepted`}
                preventScrollReset={true}
                className={() =>
                  `rounded-full px-4 py-1.5 hover:cursor-pointer hover:bg-white ${txFilter === "accepted" ? "bg-white" : ""}`
                }
              >
                Accepted
              </NavLink>
            </div>
          )}
        </div>

        {isTabActive("transactions") && (
          <div className="w-full">
            {filteredTransactions && filteredTransactions.length > 0 ? (
              <>
                <PageTable
                  alignTop
                  headers={["Timestamp", "ID", "From", "", "To", "Amount", "Status"]}
                  className="w-full md:text-sm lg:text-base"
                  additionalClassNames={{
                    0: "md:w-36 lg:w-44 whitespace-nowrap",
                    1: "md:w-72 lg:w-[20rem]",
                    2: "md:w-[20rem] lg:w-[22rem] md:ps-2 lg:ps-2 whitespace-nowrap",
                    3: "hidden md:table-cell md:w-4 lg:w-5 text-center",
                    4: "md:w-[20rem] lg:w-[22rem] whitespace-nowrap",
                    5: "md:w-28 lg:w-32 text-right whitespace-nowrap",
                    6: "md:w-16 lg:w-20 whitespace-nowrap",
                  }}
                  rowClassName={(index) => (index % 2 === 1 ? "bg-gray-25" : "")}
                  rows={(filteredTransactions || []).map((transaction) => [
                    <Tooltip
                      message={dayjs(transaction.block_time).format("MMM D, YYYY h:mm A")}
                      display={TooltipDisplayMode.Hover}
                    >
                      {dayjs(transaction.block_time).fromNow()}
                    </Tooltip>,
                    <KasLink shorten linkType="transaction" link to={transaction.transaction_id} mono />,
                    (transaction.inputs || []).length > 0 ? (
                      <ul className="leading-tight whitespace-nowrap">
                        {(transaction.inputs || [])
                          .slice(0, expand.indexOf(transaction.transaction_id) === -1 ? 5 : undefined)
                          .map(
                            (input) =>
                              input.previous_outpoint_address && (
                                <li>
                                  <KasLink
                                    link={input.previous_outpoint_address !== loaderData.address}
                                    linkType="address"
                                    to={input.previous_outpoint_address}
                                    shorten
                                    resolveName
                                    mono
                                    className="whitespace-nowrap break-normal"
                                  />
                                </li>
                              ),
                          )}
                        {(transaction.inputs || []).length > 5 && expand.indexOf(transaction.transaction_id) === -1 && (
                          <span
                            className="text-link cursor-pointer hover:underline"
                            onClick={() => setExpand((expand) => expand.concat(transaction.transaction_id))}
                          >
                            Show more (+{transaction.inputs!.length - 5})
                          </span>
                        )}
                      </ul>
                    ) : (
                      <Coinbase />
                    ),
                    <ArrowRight className="inline h-4 w-4" />,
                    <ul className="leading-tight whitespace-nowrap">
                      {(transaction.outputs || []).map((output) => (
                        <li>
                          <KasLink
                            linkType="address"
                            to={output.script_public_key_address}
                            link={loaderData.address !== output.script_public_key_address}
                            shorten
                            resolveName
                            mono
                            className="whitespace-nowrap break-normal"
                          />
                        </li>
                      ))}
                    </ul>,
                    (() => {
                      const kasAmount =
                        ((transaction.inputs || []).reduce(
                          (acc, input) =>
                            acc -
                            (loaderData.address === (input.previous_outpoint_address || "")
                              ? input.previous_outpoint_amount || 0
                              : 0),
                          0,
                        ) +
                          (transaction.outputs || []).reduce(
                            (acc, output) =>
                              acc + (loaderData.address === output.script_public_key_address ? output.amount : 0),
                            0,
                          )) /
                        1_0000_0000;
                      const amountColor = kasAmount >= 0 ? "#70C7BA" : "#C7707D";
                      const usdValue = kasAmount * (marketData?.price || 0);
                      return (
                        <>
                          <div className="text-right text-nowrap" style={{ color: amountColor }}>
                            {numeral(kasAmount).format("+0,0.00[000000]")}
                            <span className="text-nowrap"> KAS</span>
                          </div>
                          <div className="text-xs text-right text-nowrap" style={{ color: amountColor }}>
                            {numeral(usdValue).format("$0,0.00")}
                          </div>
                        </>
                      );
                    })(),
                    <span className="text-sm">{transaction.is_accepted ? <Accepted /> : <NotAccepted />}</span>,
                  ])}
                />
                <div className="ms-auto me-5 flex flex-row justify-center items-center">
                  {!isLoadingTxCount && (
                    <PageSelector
                      currentPage={currentPage}
                      totalPages={Math.ceil(txCount!.total / pageSize)}
                      onPageChange={pageChange}
                    />
                  )}
                </div>
              </>
            ) : (
              <IconMessageBox
                icon="data"
                title="No Transactions"
                description="This address doesn't have any transactions at the moment."
              />
            )}
          </div>
        )}

        {isTabActive("utxos") && (
          <>
            {(utxoData?.length ?? 0) > 0 ? (
              <>
                <PageTable
                  rows={(utxoData?.slice(0, 50) || []).map((utxo) => [
                    utxoTxTimeById.has(utxo.outpoint.transactionId) ? (
                      <Tooltip
                        message={dayjs(utxoTxTimeById.get(utxo.outpoint.transactionId) as number).format(
                          "MMM D, YYYY h:mm A",
                        )}
                        display={TooltipDisplayMode.Hover}
                      >
                        {dayjs(utxoTxTimeById.get(utxo.outpoint.transactionId) as number).fromNow()}
                      </Tooltip>
                    ) : (
                      "--"
                    ),
                    utxo.utxoEntry.blockDaaScore,
                    <KasLink linkType="transaction" to={utxo.outpoint.transactionId} link />,
                    utxo.outpoint.index,
                    numeral(parseFloat(utxo.utxoEntry.amount) / 1_0000_0000).format("0,0.00[000000]") + " KAS",
                  ])}
                  headers={["Timestamp", "Block DAA Score", "Transaction ID", "Index", "Amount"]}
                />
                {utxoData?.slice(0, 50).length === 50 && (
                  <div className="me-auto ms-auto">
                    There are more than 50 UTXOs for this address, which are not displayed.
                  </div>
                )}
              </>
            ) : (
              <IconMessageBox
                icon="data"
                title="No UTXOs"
                description="This address doesn't have any UTXOs at the moment."
              />
            )}
          </>
        )}
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

const FieldName = ({ name, infoText }: { name: string; infoText?: string }) => (
  <div className="flex flex-row items-start fill-gray-500 text-gray-500 sm:col-start-1">
    <div className="flex flex-row items-center">
      <Tooltip message={infoText || ""} display={TooltipDisplayMode.Hover} multiLine>
        <Info className="h-4 w-4" />
      </Tooltip>
      <span className="ms-1">{name}</span>
    </div>
  </div>
);

const FieldValue = ({ value }: { value: string | React.ReactNode }) => <span>{value}</span>;
