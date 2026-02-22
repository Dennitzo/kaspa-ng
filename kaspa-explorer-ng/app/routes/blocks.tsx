import KasLink from "../KasLink";
import PageTable from "../PageTable";
import Box from "../assets/box.svg";
import { useBlockdagInfo } from "../hooks/useBlockDagInfo";
import { useBlockReward } from "../hooks/useBlockReward";
import { type Block, useIncomingBlocks } from "../hooks/useIncomingBlocks";
import { useSocketCommand } from "../hooks/useSocketCommand";
import { useTransactionsCount } from "../hooks/useTransactionsCount";
import Card from "../layout/Card";
import CardContainer from "../layout/CardContainer";
import FooterHelper from "../layout/FooterHelper";
import HelperBox from "../layout/HelperBox";
import MainBox from "../layout/MainBox";
import dayjs from "dayjs";
import localeData from "dayjs/plugin/localeData";
import localizedFormat from "dayjs/plugin/localizedFormat";
import relativeTime from "dayjs/plugin/relativeTime";
import numeral from "numeral";
import { useEffect, useRef, useState } from "react";

dayjs().locale("en");
dayjs.extend(relativeTime);
dayjs.extend(localeData);
dayjs.extend(localizedFormat);

export function meta() {
  return [
    { title: "Kaspa Blocks List | Kaspa Explorer" },
    {
      name: "description",
      content:
        "Explore recent Kaspa blocks. View height, timestamp, transactions, block hash, and miner details in real-time.",
    },
    { name: "keywords", content: "Kaspa blocks, blockchain explorer, latest blocks, transactions, miners" },
  ];
}

export default function Blocks() {
  const { data: blockDagInfo } = useBlockdagInfo();
  const { data: blockReward, isLoading: isLoadingBlockReward } = useBlockReward();
  const { isLoading: isLoadingTxCount } = useTransactionsCount();

  const [blocks, setBlocks] = useState<Block[]>([]);
  const [avgBpsSinceOpen, setAvgBpsSinceOpen] = useState(0);
  const [totalBlocksSinceOpen, setTotalBlocksSinceOpen] = useState(0);
  const [totalTxSinceOpen, setTotalTxSinceOpen] = useState(0);
  const statsStartedAtRef = useRef<number | null>(null);
  const seenBlockHashesRef = useRef<Set<string>>(new Set());

  const { blocks: incomingBlocks, avgBlockTime } = useIncomingBlocks();

  useEffect(() => {
    if (!incomingBlocks.length) return;

    const now = Date.now();
    if (statsStartedAtRef.current === null) {
      statsStartedAtRef.current = now;
      seenBlockHashesRef.current = new Set(incomingBlocks.map((block) => block.block_hash));
      setAvgBpsSinceOpen(0);
      setTotalBlocksSinceOpen(0);
      setTotalTxSinceOpen(0);
      return;
    }

    let newBlocks = 0;
    let newTxCount = 0;
    for (const block of incomingBlocks) {
      if (!seenBlockHashesRef.current.has(block.block_hash)) {
        seenBlockHashesRef.current.add(block.block_hash);
        newBlocks += 1;
        newTxCount += block.txCount || 0;
      }
    }

    if (newBlocks > 0) {
      const elapsedSeconds = Math.max((now - statsStartedAtRef.current) / 1000, 1);
      setTotalBlocksSinceOpen((prev) => {
        const next = prev + newBlocks;
        setAvgBpsSinceOpen(next / elapsedSeconds);
        return next;
      });
      setTotalTxSinceOpen((prev) => prev + newTxCount);
    }
  }, [incomingBlocks]);

  useEffect(() => {
    if (!incomingBlocks.length) return;
    setBlocks((prev) => {
      const seen = new Set(prev.map((block) => block.block_hash));
      const merged = [...incomingBlocks.filter((block) => !seen.has(block.block_hash)), ...prev];
      return merged.slice(0, 20);
    });
  }, [incomingBlocks]);

  useSocketCommand({
    command: "last-blocks",
    onReceive: (data: Block[]) => {
      setBlocks(data.reverse());
    },
  });

  const displayedBlocks = blocks.slice(0, 10);

  return (
    <>
      <MainBox>
        <CardContainer title="Blocks">
          <Card title="Total blocks" value={`${numeral(totalBlocksSinceOpen).format("0,0")}`} />
          <Card loading={isLoadingTxCount} title="Total transactions" value={numeral(totalTxSinceOpen).format("0,0")} />
          <Card
            title="Average block time"
            value={`${numeral(avgBpsSinceOpen || avgBlockTime).format("0.0")} bps`}
          />
          <Card
            loading={isLoadingBlockReward}
            title="Block rewards"
            value={`${numeral(blockReward?.blockreward).format("0.00")} KAS`}
          />
        </CardContainer>
      </MainBox>

      <div className="flex w-full flex-col rounded-4xl bg-white p-4 text-left sm:p-8">
        <HelperBox>
          Blocks are arriving with a speed of 10 blocks per second. The network is currently at block{" "}
          {numeral(blockDagInfo?.virtualDaaScore).format("0,0")}.
        </HelperBox>

        <PageTable
          className="text-black"
          headers={["Timestamp", "Hash", "BlueScore", "TX Count"]}
          additionalClassNames={{ 1: "overflow-hidden " }}
          rowKeys={displayedBlocks.map((block) => block.block_hash)}
          rows={displayedBlocks.map((block) => [
            dayjs(parseInt(block.timestamp)).format("HH:mm:ss"),
            <KasLink linkType="block" link to={block.block_hash} mono />,
            block.blueScore,
            block.txCount,
          ])}
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
