import { displayAcceptance } from "../Accepted";
import Coinbase from "../Coinbase";
import Button from "../Button";
import ErrorMessage from "../ErrorMessage";
import IconMessageBox from "../IconMessageBox";
import KasLink from "../KasLink";
import LoadingMessage from "../LoadingMessage";
import PageTable from "../PageTable";
import Tooltip, { TooltipDisplayMode } from "../Tooltip";
import InfoIcon from "../assets/info.svg";
import Kaspa from "../assets/kaspa.svg";
import Swap from "../assets/swap.svg";
import Transaction from "../assets/transaction.svg";
import { MarketDataContext } from "../context/MarketDataProvider";
import { useTransactionById } from "../hooks/useTransactionById";
import { useTransactionCount } from "../hooks/useTransactionCount";
import { useVirtualChainBlueScore } from "../hooks/useVirtualChainBlueScore";
import FooterHelper from "../layout/FooterHelper";
import type { Route } from "./+types/transactiondetails";
import dayjs from "dayjs";
import localeData from "dayjs/plugin/localeData";
import localizedFormat from "dayjs/plugin/localizedFormat";
import relativeTime from "dayjs/plugin/relativeTime";
import numeral from "numeral";
import { useContext, useLayoutEffect, useRef, useState } from "react";
import { NavLink, useLocation } from "react-router";

dayjs().locale("en");
dayjs.extend(relativeTime);
dayjs.extend(localeData);
dayjs.extend(localizedFormat);

export function meta({ params }: Route.LoaderArgs) {
  return [
    { title: `Kaspa Transaction ${params.transactionId} | Kaspa Explorer` },
    {
      name: "description",
      content: "Explore Kaspa transaction. View sender, recipient, amount, status, and associated blocks and more.",
    },
    {
      name: "keywords",
      content: "Kaspa transaction, transaction ID, blockchain transfer, sender, receiver, transaction status",
    },
  ];
}

export default function TransactionDetails({ params }: Route.ComponentProps) {
  const transactionId = params.transactionId ?? "";
  const location = useLocation();
  const isTabActive = (tab: string) => (new URLSearchParams(location.search).get("tab") || "general") === tab;
  const { virtualChainBlueScore } = useVirtualChainBlueScore();

  const { data: transaction, isLoading, isError, error, refetch, isFetching } = useTransactionById(transactionId);
  const marketData = useContext(MarketDataContext);
  const [graphMode, setGraphMode] = useState<"minimal" | "detailed">("minimal");
  const flowContainerRef = useRef<HTMLDivElement>(null);
  const flowTooltipRef = useRef<HTMLDivElement>(null);
  const [flowHover, setFlowHover] = useState<{ text: string; x: number; y: number } | null>(null);
  const [flowActiveKey, setFlowActiveKey] = useState<string | null>(null);
  const [flowHoverPos, setFlowHoverPos] = useState<{ x: number; y: number } | null>(null);

  useLayoutEffect(() => {
    if (!flowHover || !flowContainerRef.current || !flowTooltipRef.current) return;
    const container = flowContainerRef.current.getBoundingClientRect();
    const tip = flowTooltipRef.current.getBoundingClientRect();
    const pad = 8;
    let x = flowHover.x;
    let y = flowHover.y;
    if (x + tip.width + pad > container.width) x = container.width - tip.width - pad;
    if (y + tip.height + pad > container.height) y = container.height - tip.height - pad;
    x = Math.max(pad, x);
    y = Math.max(pad, y);
    setFlowHoverPos({ x, y });
  }, [flowHover]);

  if (!transactionId) {
    return (
      <ErrorMessage>
        The requested transaction could not be found. Please verify the transaction ID and try again.
      </ErrorMessage>
    );
  }

  if (isLoading) {
    return <LoadingMessage>Fetching transaction details...</LoadingMessage>;
  }

  // type guard transaction
  if (isError || !transaction) {
    return (
      <ErrorMessage>
        The requested transaction could not be found. It may not be indexed yet. Please verify the transaction ID and
        try again.
        <div className="mt-4 flex flex-col items-center gap-2">
          {error && <span className="text-xs text-gray-500">{error instanceof Error ? error.message : String(error)}</span>}
          <Button
            className="h-10"
            value={isFetching ? "Retrying..." : "Retry"}
            primary
            onClick={() => refetch()}
          />
        </div>
      </ErrorMessage>
    );
  }

  const confirmations = (virtualChainBlueScore ?? 0) - (transaction?.accepting_block_blue_score || 0);
  const transactionSum = (transaction.outputs || []).reduce((sum, output) => sum + output.amount, 0);
  const displayKAS = (x: number) => numeral((x || 0) / 1_0000_0000).format("0,0.00[000000]");
  const displaySum = displayKAS(transactionSum);
  const inputSum = transaction?.inputs?.reduce((sum, input) => sum + input.previous_outpoint_amount, 0) || 0;
  const feeAmountAtomic = Math.max(0, inputSum - transactionSum);
  const outputTooltip = `Total outputs: ${displayKAS(transactionSum)} KAS`;
  const feeTooltip = `Fee: ${displayKAS(feeAmountAtomic)} KAS`;
  const isCoinbase = !transaction.inputs || transaction.inputs.length === 0;
  const allInputItems = (transaction.inputs || []).map((input) => ({
    address: input.previous_outpoint_address,
    amount: input.previous_outpoint_amount || 0,
  }));
  const minimalLimit = 10;
  const extraInputCount = Math.max(0, allInputItems.length - (minimalLimit - 1));
  const extraInputAmount = allInputItems
    .slice(minimalLimit - 1)
    .reduce((sum, input) => sum + input.amount, 0);
  const inputItems = allInputItems.slice(0, extraInputCount > 0 ? minimalLimit - 1 : minimalLimit);
  const inputItemsWithOverflow =
    extraInputCount > 0
      ? [
          ...inputItems,
          { address: `+${extraInputCount} Inputs`, amount: extraInputAmount, isOverflow: true },
        ]
      : inputItems;
  const allOutputItems = (transaction.outputs || []).map((output) => ({
    address: output.script_public_key_address,
    amount: output.amount || 0,
  }));
  const extraOutputCount = Math.max(0, allOutputItems.length - (minimalLimit - 1));
  const extraOutputAmount = allOutputItems
    .slice(minimalLimit - 1)
    .reduce((sum, output) => sum + output.amount, 0);
  const outputItems = allOutputItems.slice(0, extraOutputCount > 0 ? minimalLimit - 1 : minimalLimit);
  const outputItemsWithOverflow =
    extraOutputCount > 0
      ? [
          ...outputItems,
          { address: `+${extraOutputCount} Outputs`, amount: extraOutputAmount, isOverflow: true },
        ]
      : outputItems;
  const outputNodes = [...allOutputItems, { address: "Fee", amount: feeAmountAtomic, isFee: true }].filter(
    (item) => item.amount > 0,
  );
  const isDetailed = graphMode === "detailed";
  const inputGraphItems = isCoinbase
    ? [{ address: "COINBASE", amount: transactionSum, isCoinbase: true }]
    : isDetailed
      ? allInputItems
      : inputItemsWithOverflow;
  const outputGraphItems = isDetailed
    ? outputNodes
    : [...outputItemsWithOverflow, { address: "Fee", amount: feeAmountAtomic, isFee: true }].filter(
        (item) => item.amount > 0,
      );
  const inputCount = inputGraphItems.length || 1;
  const inputTotalForGraph = isCoinbase ? transactionSum : inputSum;
  const renderOutputs = [...outputGraphItems].sort((a, b) => (a.isFee ? -1 : 1) - (b.isFee ? -1 : 1));
  const outputCount = renderOutputs.length || 1;
  const maxFlowCount = Math.max(inputCount, outputCount, 1);
  const minDotSpacing = 12;
  const flowTop = 30;
  const flowBottom = flowTop + Math.max(240, (maxFlowCount - 1) * minDotSpacing);
  const flowHeight = flowBottom + 80;
  const hubY = flowTop + (flowBottom - flowTop) * 0.45;
  const yFor = (index: number, count: number) =>
    count === 1 ? (flowTop + flowBottom) / 2 : flowTop + (flowBottom - flowTop) * (index / (count - 1));
  const strokeFor = (amount: number, total: number, min: number, max: number) =>
    total > 0 ? Math.max(min, max * (amount / total)) : min;
  const handleFlowHover = (event: React.MouseEvent<SVGElement | HTMLDivElement>, text: string, key: string) => {
    if (!flowContainerRef.current) return;
    const rect = flowContainerRef.current.getBoundingClientRect();
    setFlowHover({ text, x: event.clientX - rect.left + 12, y: event.clientY - rect.top + 12 });
    setFlowActiveKey(key);
  };
  const clearFlowHover = () => {
    setFlowHover(null);
    setFlowActiveKey(null);
    setFlowHoverPos(null);
  };

  const flowColors = {
    input: { base: "#b9e3dd", hover: "#c7f2ea" },
    output: { base: "#70C7BA", hover: "#c7f2ea" },
    fee: { base: "#f7931a", hover: "#ffd08a" },
    wall: { base: "#e5e7eb", hover: "#c7f2ea" },
  };

  const blockTime = dayjs(transaction?.block_time);

  const fee = (inputSum - transactionSum) / 1_0000_0000;

  return (
    <>
      <div className="flex w-full flex-col rounded-4xl bg-white p-4 text-left text-black sm:p-8">
        <div className="flex flex-row items-center text-2xl sm:col-span-2">
          <Swap className="mr-2 h-8 w-8" />
          <span>Transaction details</span>
        </div>

        <span className="mt-4 mb-0">Transfer details</span>

        <span className="flex flex-row items-center text-[32px]">
          {displaySum.split(".")[0]}.<span className="self-end pb-[0.4rem] text-2xl">{displaySum.split(".")[1]}</span>
          <Kaspa className="fill-primary ml-1 h-8 w-8" />
        </span>
        <span className="ml-1 text-gray-500">
          {numeral(((transactionSum || 0) / 1_0000_0000) * (marketData?.price || 0)).format("$0,0.00")}
        </span>
        {/*horizontal rule*/}
        <div className={`my-4 h-[1px] bg-gray-100 sm:col-span-2`} />

        <div className="grid grid-cols-1 gap-x-14 gap-y-2 sm:grid-cols-[auto_1fr]">
          <FieldName name="From" infoText="The (input) address(es) that sent KAS in this transaction." />
          <FieldValue
            value={
              <ul>
                {transaction.inputs ? (
                  [...new Set(transaction.inputs.map((input) => input.previous_outpoint_address))].map((addr) => (
                    <li>
                      <KasLink linkType="address" copy link to={addr} />
                    </li>
                  ))
                ) : (
                  <li>
                    <Coinbase />
                  </li>
                )}
              </ul>
            }
          />
          <FieldName name="To" infoText="The (output) address(es) where the KAS in this transaction were sent to." />
          <FieldValue
            value={
              <ul>
                {transaction.outputs ? (
                  [...new Set(transaction.outputs.map((output) => output.script_public_key_address))].map((addr) => (
                    <li>
                      <KasLink linkType="address" copy link to={addr} resolveName />
                    </li>
                  ))
                ) : (
                  <span>No output addresses</span>
                )}
              </ul>
            }
          />
        </div>
      </div>

      {(inputSum > 0 || isCoinbase) && (
        <div className="flex w-full flex-col rounded-4xl bg-white p-4 text-left text-black sm:p-8">
          <div className="flex flex-col gap-3 sm:col-span-2 sm:flex-row sm:items-center sm:justify-between">
            <div className="flex items-center text-2xl">
              <Swap className="mr-2 h-8 w-8" />
              <span>Transaction flow</span>
            </div>
            <div className="flex w-auto flex-row items-center justify-around gap-x-1 rounded-full bg-gray-50 p-1 px-1 text-sm">
              <button
                type="button"
                onClick={() => setGraphMode("minimal")}
                className={`rounded-full px-4 py-1.5 hover:cursor-pointer hover:bg-white ${graphMode === "minimal" ? "bg-white" : ""}`}
              >
                Minimal graph
              </button>
              <button
                type="button"
                onClick={() => setGraphMode("detailed")}
                className={`rounded-full px-4 py-1.5 hover:cursor-pointer hover:bg-white ${graphMode === "detailed" ? "bg-white" : ""}`}
              >
                Detailed graph
              </button>
            </div>
          </div>

          <div
            ref={flowContainerRef}
            className="relative mt-6 w-full"
            style={{ height: `${flowHeight}px` }}
            onMouseLeave={clearFlowHover}
          >
            <svg className="absolute inset-0 h-full w-full" viewBox={`0 0 1000 ${flowHeight}`} preserveAspectRatio="none">
              <defs>
                <marker id="flow-arrow-output" viewBox="0 0 12 12" refX="8" refY="6" markerWidth="4" markerHeight="4" orient="auto">
                  <path d="M 0 0 L 12 6 L 0 12 z" fill="url(#flow-gradient)" />
                </marker>
                <marker
                  id="flow-arrow-output-hover"
                  viewBox="0 0 12 12"
                  refX="8"
                  refY="6"
                  markerWidth="4"
                  markerHeight="4"
                  orient="auto"
                >
                  <path d="M 0 0 L 12 6 L 0 12 z" fill="#c7f2ea" />
                </marker>
                <marker id="flow-arrow-fee" viewBox="0 0 12 12" refX="8" refY="6" markerWidth="4" markerHeight="4" orient="auto">
                  <path d="M 0 0 L 12 6 L 0 12 z" fill="#f7931a" />
                </marker>
                <marker
                  id="flow-arrow-fee-hover"
                  viewBox="0 0 12 12"
                  refX="8"
                  refY="6"
                  markerWidth="4"
                  markerHeight="4"
                  orient="auto"
                >
                  <path d="M 0 0 L 12 6 L 0 12 z" fill="#ffd08a" />
                </marker>
                <linearGradient id="flow-gradient" x1="0" y1="0" x2="1" y2="0">
                  <stop offset="0%" stopColor="#70C7BA" />
                  <stop offset="100%" stopColor="#49EACB" />
                </linearGradient>
                <linearGradient id="flow-gradient-hover" x1="0" y1="0" x2="1" y2="0">
                  <stop offset="0%" stopColor="#c7f2ea" />
                  <stop offset="100%" stopColor="#c7f2ea" />
                </linearGradient>
              </defs>
              {renderOutputs.map((output, index) => {
                const y = yFor(index, outputCount);
                const baseStrokeWidth = strokeFor(output.amount, transactionSum + feeAmountAtomic, 4, 18);
                const label = output.isOverflow
                  ? `${output.address} • ${displayKAS(output.amount)} KAS`
                  : output.isFee
                    ? feeTooltip
                    : `${output.address} • ${displayKAS(output.amount)} KAS`;
                const key = `out-${index}`;
                return (
                  <path
                    key={`out-path-${index}`}
                    d={`M 500 ${hubY} C 640 ${hubY + (y - hubY) * 0.35}, 760 ${y}, 880 ${y}`}
                    fill="none"
                    stroke={
                      output.isFee
                        ? flowActiveKey === key
                          ? flowColors.fee.hover
                          : flowColors.fee.base
                        : flowActiveKey === key
                          ? flowColors.output.hover
                          : "url(#flow-gradient)"
                    }
                    strokeWidth={flowActiveKey === key ? baseStrokeWidth + 2 : baseStrokeWidth}
                    strokeLinecap="round"
                    markerEnd={
                      output.isFee
                        ? flowActiveKey === key
                          ? "url(#flow-arrow-fee-hover)"
                          : "url(#flow-arrow-fee)"
                        : flowActiveKey === key
                          ? "url(#flow-arrow-output-hover)"
                          : "url(#flow-arrow-output)"
                    }
                    onMouseEnter={(event) => handleFlowHover(event, label, key)}
                    onMouseLeave={clearFlowHover}
                  />
                );
              })}
              {inputGraphItems.map((input, index) => {
                const y = yFor(index, inputCount);
                const baseStrokeWidth = strokeFor(input.amount, inputTotalForGraph, 4, 16);
                const label = input.isOverflow
                  ? `${input.address} • ${displayKAS(input.amount)} KAS`
                  : input.isCoinbase
                    ? `COINBASE • ${displayKAS(input.amount)} KAS`
                    : `${input.address} • ${displayKAS(input.amount)} KAS`;
                const key = `in-${index}`;
                return (
                  <path
                    key={`in-path-${index}`}
                    d={`M 120 ${y} C 260 ${y}, 360 ${y + (hubY - y) * 0.35}, 500 ${hubY}`}
                    fill="none"
                    stroke={flowActiveKey === key ? flowColors.input.hover : "url(#flow-gradient)"}
                    strokeWidth={flowActiveKey === key ? baseStrokeWidth + 2 : baseStrokeWidth}
                    strokeLinecap="round"
                    onMouseEnter={(event) => handleFlowHover(event, label, key)}
                    onMouseLeave={clearFlowHover}
                  />
                );
              })}
            </svg>

            <div className="pointer-events-none absolute left-0 top-0 h-full w-full">
              {inputGraphItems.map((input, index) => {
                const y = yFor(index, inputCount);
                const label = input.isOverflow
                  ? `${input.address} • ${displayKAS(input.amount)} KAS`
                  : input.isCoinbase
                    ? `COINBASE • ${displayKAS(input.amount)} KAS`
                    : `${input.address} • ${displayKAS(input.amount)} KAS`;
                const key = `in-${index}`;
                return (
                  <div
                    key={`in-node-${index}`}
                    className="pointer-events-auto absolute flex items-center -translate-y-1/2"
                    style={{ left: "3%", top: y }}
                    onMouseEnter={(event) => handleFlowHover(event, label, key)}
                    onMouseLeave={clearFlowHover}
                  >
                    <span className="mr-2 w-20 text-left text-xs text-gray-500">
                      {!isDetailed && extraInputCount > 0 && input.isOverflow
                        ? `+${extraInputCount} Inputs`
                        : input.isCoinbase
                          ? "COINBASE"
                          : `Input #${inputGraphItems
                              .slice(0, index)
                              .filter((item) => !item.isCoinbase && !item.isOverflow).length + 1}`}
                    </span>
                    <div
                      className={`rounded-full shadow-sm ${flowActiveKey === key ? "h-3.5 w-3.5" : "h-2.5 w-2.5"}`}
                      style={{
                        backgroundColor: flowActiveKey === key ? flowColors.input.hover : flowColors.output.base,
                      }}
                    />
                  </div>
                );
              })}
              <div className="pointer-events-auto absolute" style={{ left: "50%", top: `${(hubY / flowHeight) * 100}%` }}>
                <div
                  className="h-6 w-6 -translate-x-1/2 -translate-y-1/2 rounded-full shadow-sm"
                  style={{ backgroundColor: flowActiveKey === "wall" ? flowColors.wall.hover : flowColors.wall.base }}
                  onMouseEnter={(event) => handleFlowHover(event, outputTooltip, "wall")}
                  onMouseLeave={clearFlowHover}
                />
              </div>
              {renderOutputs.map((output, index) => {
                const y = yFor(index, outputCount);
                const label = output.isOverflow
                  ? `${output.address} • ${displayKAS(output.amount)} KAS`
                  : output.isFee
                    ? feeTooltip
                    : `${output.address} • ${displayKAS(output.amount)} KAS`;
                const key = `out-${index}`;
                return (
                  <div
                    key={`out-node-${index}`}
                    className="pointer-events-auto absolute flex items-center -translate-y-1/2"
                    style={{ left: "94%", top: y }}
                    onMouseEnter={(event) => handleFlowHover(event, label, key)}
                    onMouseLeave={clearFlowHover}
                  >
                    <div
                      className={`rounded-full shadow-sm ${flowActiveKey === key ? "h-3.5 w-6" : "h-2.5 w-5"}`}
                      style={{
                        backgroundColor: output.isFee
                          ? flowActiveKey === key
                            ? flowColors.fee.hover
                            : flowColors.fee.base
                          : flowActiveKey === key
                            ? flowColors.output.hover
                            : flowColors.output.base,
                      }}
                    />
                    <span className="ml-2 w-20 text-left text-xs text-gray-500">
                      {!isDetailed && extraOutputCount > 0 && output.isOverflow
                        ? `+${extraOutputCount} Outputs`
                        : output.isFee
                          ? "Fee"
                          : `Output #${index + 1}`}
                    </span>
                  </div>
                );
              })}
            </div>
            {flowHover && (
              <div
                ref={flowTooltipRef}
                className="pointer-events-none absolute z-10 max-w-xs break-words whitespace-normal rounded-md bg-black/80 px-2 py-1 text-xs text-white"
                style={{ left: (flowHoverPos ?? flowHover).x, top: (flowHoverPos ?? flowHover).y }}
              >
                {flowHover.text}
              </div>
            )}
          </div>
        </div>
      )}

      <div className="flex w-full flex-col gap-x-18 gap-y-6 rounded-4xl bg-white p-4 text-left text-black sm:p-8">
        <div className="mr-auto flex w-auto flex-row items-center justify-around gap-x-1 rounded-full bg-gray-50 p-1 px-1">
          <NavLink
            to={`/transactions/${transactionId}?tab=general`}
            preventScrollReset={true}
            className={() =>
              `rounded-full px-4 py-1.5 hover:cursor-pointer hover:bg-white ${isTabActive("general") ? "bg-white" : ""}`
            }
          >
            General information
          </NavLink>
          <NavLink
            to={`/transactions/${transactionId}?tab=inputs`}
            preventScrollReset={true}
            className={() =>
              `rounded-full px-4 py-1.5 hover:cursor-pointer hover:bg-white ${isTabActive("inputs") ? "bg-white" : ""}`
            }
          >
            Inputs
          </NavLink>
          <NavLink
            to={`/transactions/${transactionId}?tab=outputs`}
            preventScrollReset={true}
            className={() =>
              `rounded-full px-4 py-1.5 hover:cursor-pointer hover:bg-white ${isTabActive("outputs") ? "bg-white" : ""}`
            }
          >
            Outputs
          </NavLink>
        </div>

        {isTabActive("general") && transaction && (
          <div className="grid w-full grid-cols-1 gap-x-18 gap-y-2 rounded-4xl bg-white text-left text-nowrap text-black sm:grid-cols-[auto_1fr]">
            <FieldName name="Transaction ID" infoText="The unique identifier of this transaction." />
            <FieldValue value={<KasLink copy to={transaction.transaction_id} linkType="transaction" />} />
            <FieldName
              name="Subnetwork ID"
              infoText="The Subnetwork ID is an identifier for transactions. It's used to group mining and regular transactions."
            />
            <FieldValue value={transaction.subnetwork_id} />
            <FieldName
              name="Status"
              infoText="Displays whether the transaction was accepted by the protocol and how many confirmations it has so far."
            />

            <FieldValue value={displayAcceptance(transaction.is_accepted, confirmations)} />
            {/*horizontal rule*/}
            <div className={`my-4 h-[1px] bg-gray-100 sm:col-span-2`} />
            <FieldName name="Hash" infoText="Hash calculated from the transaction data." />
            <FieldValue value={transaction.hash} />
            <FieldName
              name="Compute mass"
              infoText="The computed mass / weight of a transaction. It's used to determine the fee of a transaction."
            />

            <FieldValue value={inputSum === 0 ? 0 : transaction.mass} />
            {/*horizontal rule*/}
            <div className={`my-4 h-[1px] bg-gray-100 sm:col-span-2`} />
            <FieldName name="Block hashes" infoText="Blocks, in which this transaction was included." />
            <FieldValue
              value={transaction.block_hash.map((blockHash) => (
                <div>
                  <KasLink linkType="block" link to={blockHash} />
                </div>
              ))}
            />
            <FieldName name="Block time" infoText="Timestamp, when the transaction was included in a block." />
            <FieldValue
              value={
                <>
                  <div className="flex flex-col">
                    <span>{blockTime.fromNow()}</span>
                    <span className="text-gray-500">{blockTime.format("ll LTS")}</span>
                  </div>
                </>
              }
            />
            <FieldName
              name="Accepting block hash"
              infoText="Block hash of a chain block, which accepted this transaction."
            />
            <FieldValue
              value={
                transaction.accepting_block_hash ? (
                  <KasLink link linkType="block" to={transaction.accepting_block_hash} />
                ) : (
                  "Transaction is not accepted."
                )
              }
            />
            {transaction.payload && (
              <>
                <FieldName name="Payload" infoText="Payload data, which is used for miners to transmit miner's data." />
                <FieldValue
                  className="rounded-lg bg-gray-50 px-1 py-2 font-mono text-wrap break-all"
                  value={transaction.payload}
                />
              </>
            )}
            {(transaction.inputs || []).length > 0 && (
              <>
                <div className={`my-4 h-[1px] bg-gray-100 sm:col-span-2`} />
                <FieldName
                  name="Transaction fee"
                  infoText="Fee for this transaction which goes to miners as reward. It is the total output amount minus the total input amount."
                />
                <FieldValue
                  value={
                    <>
                      <span>{fee}</span>
                      <span className="text-gray-500 text-nowrap"> KAS</span>
                      <div className="text-gray-500">
                        {numeral((fee * (marketData?.price || 0)).toFixed(6)).format("$0,0.00[000000]")}
                      </div>
                    </>
                  }
                />
              </>
            )}
          </div>
        )}

        {isTabActive("inputs") && (
          <>
            {transaction.inputs && transaction.inputs.length > 0 ? (
              <div className="text-nowrap">
                <PageTable
                  rows={transaction.inputs.map((input) => {
                    return [
                      input.sig_op_count,
                      <span className="text-wrap">{input.signature_script}</span>,
                      <>
                        <KasLink linkType="transaction" link shorten to={input.previous_outpoint_hash} />
                        {` #${input.previous_outpoint_index}`}
                      </>,
                      <KasLink linkType="address" shorten link to={input.previous_outpoint_address} />,
                      <>
                        <span className="text-nowrap">
                          {displayKAS(input.previous_outpoint_amount).split(".")[0]}.
                          <span className="self-end pb-[0.4rem]">
                            {displayKAS(input.previous_outpoint_amount).split(".")[1]}
                          </span>
                        </span>
                        <span className="text-gray-500 text-nowrap"> KAS</span>
                      </>,
                    ];
                  })}
                  headers={[
                    "Sig Op Count",
                    "Signature Script",
                    <span className="text-nowrap">Outpoint ID & Index</span>,
                    "Outpoint Address",
                    "Amount",
                  ]}
                />
              </div>
            ) : (
              <div className="w-full flex flex-col items-center justify-center">
                <IconMessageBox icon="data" title="No inputs" description="This transaction has no inputs." />
              </div>
            )}
          </>
        )}

        {isTabActive("outputs") && transaction.outputs && transaction.outputs.length > 0 && (
          <PageTable
            headers={["Index", "Type", "Script Public Key", "Script Public Key Address", "Amount"]}
            rows={transaction.outputs.map((output) => {
              return [
                output.index || "0",
                <span className="text-nowrap">{output.script_public_key_type}</span>,
                output.script_public_key,
                <KasLink linkType="address" to={output.script_public_key_address} link resolveName />,
                <span className="text-nowrap">
                  <span>
                    {displayKAS(output.amount).split(".")[0]}.
                    <span className="self-end pb-[0.4rem]">{displayKAS(output.amount).split(".")[1]}</span>
                  </span>
                  <span className="text-gray-500 text-nowrap"> KAS</span>
                </span>,
              ];
            })}
          />
        )}
      </div>
      <FooterHelper icon={Transaction}>
        A transaction is a cryptographically signed command that modifies the blockchain's state. Block explorers
        monitor and display the details of every transaction within the network.
      </FooterHelper>
    </>
  );
}

const FieldName = ({ name, infoText, className }: { name: string; infoText?: string; className?: string }) => (
  <div className={`flex flex-row items-start fill-gray-500 text-gray-500 sm:col-start-1 ${className ? className : ""}`}>
    <div className="flex flex-row items-center">
      <Tooltip message={infoText || ""} display={TooltipDisplayMode.Hover} multiLine>
        <InfoIcon className="h-4 w-4" />
      </Tooltip>
      <span className="ms-1">{name}</span>
    </div>
  </div>
);

const FieldValue = ({ value, className }: { value: string | React.ReactNode; className?: string }) => (
  <span className={`text-wrap break-all ${className}`}>{value}</span>
);
