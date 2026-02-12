import { displayAcceptance } from "../Accepted";
import Coinbase from "../Coinbase";
import ErrorMessage from "../ErrorMessage";
import KasLink from "../KasLink";
import LoadingMessage from "../LoadingMessage";
import PageTable from "../PageTable";
import Tooltip, { TooltipDisplayMode } from "../Tooltip";
import ArrowRight from "../assets/arrow-right.svg";
import Box from "../assets/box.svg";
import Info from "../assets/info.svg";
import { useBlockById } from "../hooks/useBlockById";
import { useTransactionsSearch } from "../hooks/useTransactionsSearch";
import { useVirtualChainBlueScore } from "../hooks/useVirtualChainBlueScore";
import FooterHelper from "../layout/FooterHelper";
import type { Route } from "./+types/blockdetails";
import dayjs from "dayjs";
import localeData from "dayjs/plugin/localeData";
import localizedFormat from "dayjs/plugin/localizedFormat";
import relativeTime from "dayjs/plugin/relativeTime";
import numeral from "numeral";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Link } from "react-router";

dayjs().locale("en");
dayjs.extend(relativeTime);
dayjs.extend(localeData);
dayjs.extend(localizedFormat);

export function meta({ params }: Route.LoaderArgs) {
  return [
    { title: `Kaspa Block ${params.blockId} | Kaspa Explorer` },
    {
      name: "description",
      content: "View Kaspa block details. Check transactions, miner, block hash, confirmations, and DAG links.",
    },
    { name: "keywords", content: "Kaspa block, blockchain explorer, block details, miner, DAG, transactions" },
  ];
}

export default function Blocks({ params }: Route.ComponentProps) {
  const blockId = params.blockId ?? "";
  const { data: block, isLoading, isError } = useBlockById(blockId);
  const { data: inputTxs, refetch: fetchTransactions } = useTransactionsSearch(
    (block?.transactions.map((tx) => tx.verboseData.transactionId) || []).concat(
      block?.transactions.flatMap((tx) => tx.inputs.map((input) => input.previousOutpoint.transactionId)) || [],
    ),
    "",
    "light",
    false,
  );

  const { virtualChainBlueScore } = useVirtualChainBlueScore();

  const getTxFromInputTxs = (txId: string) => {
    for (const tx of inputTxs || []) {
      if (tx.transaction_id === txId) return tx;
    }
  };

  const getAddressFromOutpoint = (txId: string, outpointIndex: number) => {
    for (const tx of inputTxs || []) {
      if (tx.transaction_id === txId) {
        for (const output of tx.outputs || []) {
          if (output.index === outpointIndex) {
            return <KasLink linkType={"address"} to={output.script_public_key_address} shorten link mono />;
          }
        }
      }
    }
    return (
      <>
        <KasLink linkType={"transaction"} to={txId} shorten link mono />
        {` #${outpointIndex}`}
      </>
    );
  };

  useEffect(() => {
    if (block) {
      fetchTransactions();
    }
  }, [block]);

  useEffect(() => {
    if (block && inputTxs) {
      const cntNotAccepted = block.transactions
        .map((transaction) => getTxFromInputTxs(transaction.verboseData.transactionId)?.is_accepted ?? false)
        .filter((accepted) => !accepted).length;

      if (cntNotAccepted > 1) {
        const timeoutRefetch = setTimeout(() => fetchTransactions(), 2000);
        return () => {
          clearTimeout(timeoutRefetch);
        };
      }
    }
  }, [inputTxs]);

  const [graphMode, setGraphMode] = useState<"minimal" | "detailed">("minimal");
  const [flowHover, setFlowHover] = useState<{ text: string; x: number; y: number } | null>(null);
  const [flowActiveKey, setFlowActiveKey] = useState<string | null>(null);
  const [flowHoverPos, setFlowHoverPos] = useState<{ x: number; y: number } | null>(null);
  const flowContainerRef = useRef<HTMLDivElement>(null);
  const flowTooltipRef = useRef<HTMLDivElement>(null);

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

  const displayKAS = (amount: number) => numeral((amount || 0) / 1_0000_0000).format("0,0.00[000000]");
  const allBlockTxs = block?.transactions || [];
  const coinbaseTx = allBlockTxs.find((transaction) => transaction.inputs.length === 0);
  const blockOutputs = allBlockTxs.flatMap((transaction) =>
    transaction.outputs.map((output) => ({
      address: output.verboseData.scriptPublicKeyAddress,
      amount: output.amount || 0,
    })),
  );
  const totalOutput = blockOutputs.reduce((sum, output) => sum + output.amount, 0);
  const blockInputItems = allBlockTxs.map((transaction, index) => {
    const amount = transaction.outputs.reduce((sum, output) => sum + (output.amount || 0), 0);
    return {
      address: transaction.inputs.length === 0 ? "COINBASE" : `Tx #${index + 1}`,
      amount,
      txId: transaction.verboseData.transactionId,
      isCoinbase: transaction.inputs.length === 0,
    };
  });
  const minimalLimit = 10;
  const extraInputCount = Math.max(0, blockInputItems.length - (minimalLimit - 1));
  const extraInputAmount = blockInputItems
    .slice(minimalLimit - 1)
    .reduce((sum, input) => sum + input.amount, 0);
  const inputItems = blockInputItems.slice(0, extraInputCount > 0 ? minimalLimit - 1 : minimalLimit);
  const inputItemsWithOverflow =
    extraInputCount > 0
      ? [...inputItems, { address: `+${extraInputCount} Inputs`, amount: extraInputAmount, isOverflow: true }]
      : inputItems;

  const extraOutputCount = Math.max(0, blockOutputs.length - (minimalLimit - 1));
  const extraOutputAmount = blockOutputs
    .slice(minimalLimit - 1)
    .reduce((sum, output) => sum + output.amount, 0);
  const outputItems = blockOutputs.slice(0, extraOutputCount > 0 ? minimalLimit - 1 : minimalLimit);
  const outputItemsWithOverflow =
    extraOutputCount > 0
      ? [
          ...outputItems,
          { address: `+${extraOutputCount} Outputs`, amount: extraOutputAmount, isOverflow: true },
        ]
      : outputItems;
  const isDetailed = graphMode === "detailed";
  const inputGraphItems = isDetailed ? blockInputItems : inputItemsWithOverflow;
  const outputGraphItems = isDetailed ? blockOutputs : outputItemsWithOverflow;
  const inputCount = inputGraphItems.length || 1;
  const outputCount = outputGraphItems.length || 1;
  const maxFlowCount = Math.max(inputCount, outputCount, 1);
  const minDotSpacing = isDetailed ? 20 : 12;
  const flowTop = 30;
  const flowBottom = flowTop + Math.max(240, (maxFlowCount - 1) * minDotSpacing);
  const flowHeight = flowBottom + 80;
  const hubY = flowTop + (flowBottom - flowTop) * 0.45;
  const yFor = (index: number, count: number) =>
    count === 1 ? (flowTop + flowBottom) / 2 : flowTop + (flowBottom - flowTop) * (index / (count - 1));
  const strokeFor = (amount: number, total: number, min: number, max: number) =>
    total > 0 ? Math.max(min, max * (amount / total)) : min;
  const flowColors = {
    input: { base: "#b9e3dd", hover: "#c7f2ea" },
    output: { base: "#70C7BA", hover: "#c7f2ea" },
    wall: { base: "#e5e7eb", hover: "#c7f2ea" },
  };

  const blockTime = dayjs(Number(block?.header.timestamp));
  if (isLoading) {
    return <LoadingMessage>Loading block</LoadingMessage>;
  }

  if (isError || !blockId) {
    return (
      <ErrorMessage>
        Unable to load a block with hash
        <span className="text-gray-500"> {blockId}</span>
      </ErrorMessage>
    );
  }
  return (
    <>
      <div className="grid w-full grid-cols-1 gap-x-18 gap-y-2 rounded-4xl bg-white p-4 text-left text-nowrap text-black sm:grid-cols-[auto_1fr] sm:p-8">
        <div className="flex flex-row items-center text-2xl sm:col-span-2">
          <Box className="mr-2 h-8 w-8" />
          Blocks details
        </div>
        <div className="mt-4 text-black sm:col-span-2">Main information</div>
        <FieldName
          name="Block Hash"
          infoText="A unique identifier for this block, generated by hashing its contents."
        />
        <FieldValue value={<KasLink linkType="block" copy to={blockId} />} />
        <FieldName name="Blue score" infoText="Amount of the blue blocks so far." />
        <FieldValue value={block?.header.blueScore} />
        <FieldName name="Bits" infoText="Number of bits in the block." />
        <FieldValue value={block?.header.bits} />
        <FieldName name="Timestamp" infoText="The time when the block was created, set by the miner." />
        <FieldValue
          value={
            <div className="flex flex-col">
              <span>{blockTime.fromNow()}</span>
              <span className="text-gray-500">{blockTime.format("ll LTS")}</span>
            </div>
          }
        />
        <FieldName name="Version" infoText="Version of the block template." />
        <FieldValue value={block?.header.version} />
        <FieldName name="Is chain block" infoText="Indicates if this block is a part of the virtual chain." />
        <FieldValue value={block?.verboseData.isChainBlock ? "Yes" : "No"} />
        {/*horizontal rule*/}
        <div className={`my-4 h-[1px] bg-gray-100 sm:col-span-2`} />
        <div className="text-black sm:col-span-2">Connections</div>
        <FieldName name="Parents" infoText="Displays the parents of this block in the BlockDAG." />
        <FieldValue
          value={block?.header.parents[0].parentHashes.map((parentHash) => (
            <div>
              <KasLink linkType="block" link to={`${parentHash}`} mono />
            </div>
          ))}
        />
        <FieldName name="Children" infoText="Displays the children of this block in the BlockDAG." />
        <FieldValue
          value={block?.verboseData.childrenHashes.map((child) => (
            <div>
              <KasLink linkType="block" link to={`${child}`} mono />
            </div>
          ))}
        />
        <div className={`my-4 h-[1px] bg-gray-100 sm:col-span-2`} />
        <div className="text-black sm:col-span-2">Merkle and UTXO data</div>
        <FieldName name="Merkle root" infoText="A cryptographic hash that represents all transactions in the block." />
        <FieldValue value={block?.header.hashMerkleRoot} />
        <FieldName name="Accepted merkle root" infoText="Acceptance merkle root, used to verify the block." />
        <FieldValue value={block?.header.acceptedIdMerkleRoot} />
        <FieldName name="UTXO commitment" infoText="The block commitment to the UTXO set, used to verify the block." />
        <FieldValue value={block?.header.utxoCommitment} />
        <div className={`my-4 h-[1px] bg-gray-100 sm:col-span-2`} />
        <div className="flex flex-row items-start text-black sm:col-span-2">Difficulty and computation</div>
        <FieldName name="Nonce" infoText="A random number used to generate the block hash." />
        <FieldValue value={block?.header.nonce} />
        <FieldName name="DAA score" infoText="The count of all blocks ( red and blue ) in the network." />
        <FieldValue value={block?.header.daaScore} />
        <FieldName name="Blue work" infoText="The cumulative proof-of-work of all blue blocks." />
        <FieldValue value={block?.header.blueWork} />
        <div className={`my-4 h-[1px] bg-gray-100 sm:col-span-2`} />
        <div className="text-black sm:col-span-2">Additional data</div>
        <FieldName
          name="Pruning point"
          infoText="A reference block in the past of the BlockDAG, used to prune previous data."
        />
        <FieldValue value={<KasLink linkType="block" link to={block?.header.pruningPoint || ""} />} />
        {block?.extra?.minerInfo && (
          <>
            <FieldName
              name="Miner info"
              infoText="Miner address and free text field filled by the miner of this block."
            />
            <FieldValue
              value={
                <>
                  <div className="text-link">
                    <Link to={`/addresses/${block.extra.minerAddress}`}>{block.extra.minerAddress}</Link>
                  </div>
                  <div className="text-gray-500">{block.extra.minerInfo}</div>
                </>
              }
            />
          </>
        )}
      </div>

      {allBlockTxs.length > 0 && (
        <div className="flex w-full flex-col rounded-4xl bg-white p-4 text-left text-black sm:p-8">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
            <div className="flex items-center text-2xl">
              <Box className="mr-2 h-8 w-8" />
              <span>Block flow</span>
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
                <marker
                  id="coinbase-arrow-output"
                  viewBox="0 0 12 12"
                  refX="8"
                  refY="6"
                  markerWidth="4"
                  markerHeight="4"
                  orient="auto"
                >
                  <path d="M 0 0 L 12 6 L 0 12 z" fill="url(#coinbase-flow-gradient)" />
                </marker>
                <marker
                  id="coinbase-arrow-output-hover"
                  viewBox="0 0 12 12"
                  refX="8"
                  refY="6"
                  markerWidth="4"
                  markerHeight="4"
                  orient="auto"
                >
                  <path d="M 0 0 L 12 6 L 0 12 z" fill="#c7f2ea" />
                </marker>
                <linearGradient id="coinbase-flow-gradient" x1="0" y1="0" x2="1" y2="0">
                  <stop offset="0%" stopColor="#70C7BA" />
                  <stop offset="100%" stopColor="#49EACB" />
                </linearGradient>
                <linearGradient id="coinbase-flow-gradient-hover" x1="0" y1="0" x2="1" y2="0">
                  <stop offset="0%" stopColor="#c7f2ea" />
                  <stop offset="100%" stopColor="#c7f2ea" />
                </linearGradient>
              </defs>
              {outputGraphItems.map((output, index) => {
                const y = yFor(index, outputCount);
                const baseStrokeWidth = strokeFor(output.amount, totalOutput, 4, 18);
                const label = output.isOverflow
                  ? `${output.address} • ${displayKAS(output.amount)} KAS`
                  : `Output: ${displayKAS(output.amount)} KAS • ${output.address}`;
                const key = `out-${index}`;
                const isHover = flowActiveKey === key;
                return (
                  <path
                    key={`out-path-${index}`}
                    d={`M 500 ${hubY} C 640 ${hubY + (y - hubY) * 0.35}, 760 ${y}, 880 ${y}`}
                    fill="none"
                    stroke={isHover ? flowColors.output.hover : "url(#coinbase-flow-gradient)"}
                    strokeWidth={isHover ? baseStrokeWidth + 2 : baseStrokeWidth}
                    strokeLinecap="round"
                    markerEnd={isHover ? "url(#coinbase-arrow-output-hover)" : "url(#coinbase-arrow-output)"}
                    onMouseEnter={(event) => handleFlowHover(event, label, key)}
                    onMouseLeave={clearFlowHover}
                  />
                );
              })}
              {inputGraphItems.map((input, index) => {
                const y = yFor(index, inputCount);
                const baseStrokeWidth = strokeFor(input.amount, totalOutput, 4, 16);
                const label = input.isOverflow
                  ? `${input.address} • ${displayKAS(input.amount)} KAS`
                  : input.isCoinbase
                    ? `COINBASE • ${displayKAS(input.amount)} KAS`
                    : `Transaction: ${input.txId} • ${displayKAS(input.amount)} KAS`;
                const key = `in-${index}`;
                const isHover = flowActiveKey === key;
                return (
                  <path
                    key={`in-path-${index}`}
                    d={`M 120 ${y} C 260 ${y}, 360 ${y + (hubY - y) * 0.35}, 500 ${hubY}`}
                    fill="none"
                    stroke={isHover ? flowColors.input.hover : "url(#coinbase-flow-gradient)"}
                    strokeWidth={isHover ? baseStrokeWidth + 2 : baseStrokeWidth}
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
                    : `Transaction: ${input.txId} • ${displayKAS(input.amount)} KAS`;
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
                              .filter((item) => !item.isCoinbase && !item.isOverflow).length}`}
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
                  onMouseEnter={(event) => handleFlowHover(event, `Total outputs: ${displayKAS(totalOutput)} KAS`, "wall")}
                  onMouseLeave={clearFlowHover}
                />
              </div>
              {outputGraphItems.map((output, index) => {
                const y = yFor(index, outputCount);
                const label = output.isOverflow
                  ? `${output.address} • ${displayKAS(output.amount)} KAS`
                  : `Output: ${displayKAS(output.amount)} KAS • ${output.address}`;
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
                        backgroundColor: flowActiveKey === key ? flowColors.output.hover : flowColors.output.base,
                      }}
                    />
                    <span className="ml-2 w-20 text-left text-xs text-gray-500">
                      {!isDetailed && extraOutputCount > 0 && output.isOverflow
                        ? `+${extraOutputCount} Outputs`
                        : `Output #${index}`}
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
      <div className="flex flex-col w-full gap-x-18 gap-y-2 rounded-4xl bg-white p-4 text-left text-nowrap text-black sm:p-8">
        <div className="mt-4 mb-2 text-black sm:col-span-2">Transactions</div>
        <PageTable
          alignTop
          headers={["Transaction ID", "From", "", "To", "Amount", "Status"]}
          rows={
            block?.transactions.map((transaction) => [
              <KasLink linkType="transaction" to={transaction.verboseData.transactionId} link shorten mono />,
              <ul>
                {transaction.inputs.length > 0 ? (
                  transaction.inputs.map((input) => (
                    <li>
                      {getAddressFromOutpoint(input.previousOutpoint.transactionId, input.previousOutpoint.index)}
                    </li>
                  ))
                ) : (
                  <Coinbase />
                )}
              </ul>,
              <ArrowRight className="inline h-4 w-4" />,
              <ul>
                {transaction.outputs.map((output) => (
                  <li>
                    <KasLink
                      linkType="address"
                      to={output.verboseData.scriptPublicKeyAddress}
                      link
                      shorten
                      resolveName
                      mono
                    />
                  </li>
                ))}
              </ul>,
              <ul>
                {transaction.outputs.map((output) => (
                  <li>
                    {numeral(output.amount / 1_0000_0000).format("0,0.00[000000]")}
                    <span className="text-gray-500 text-nowrap"> KAS</span>
                  </li>
                ))}
              </ul>,
              <div className="flex flex-row gap-x-2 justfiy-start md:justify-end">
                {displayAcceptance(
                  getTxFromInputTxs(transaction.verboseData.transactionId)?.is_accepted ?? false,
                  virtualChainBlueScore
                    ? virtualChainBlueScore -
                        (getTxFromInputTxs(transaction.verboseData.transactionId)?.accepting_block_blue_score || 0)
                    : undefined,
                )}
              </div>,
            ]) || []
          }
        />
      </div>
      <FooterHelper icon={Box}>
        <span>
          A block is a secure, sequential record in the blockchain containing verified transactions, a unique hash, and
          a reference to the previous block, ensuring data integrity.
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

const FieldValue = ({ value }: { value: string | React.ReactNode }) => (
  <div className="break-all text-wrap">{value}</div>
);
