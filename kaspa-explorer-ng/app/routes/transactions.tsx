import KasLink from "../KasLink";
import PageTable from "../PageTable";
import Transaction from "../assets/transaction.svg";
import { useFeeEstimate } from "../hooks/useFeeEstimate";
import { useIncomingBlocks } from "../hooks/useIncomingBlocks";
import { useMempoolSize } from "../hooks/useMempoolSize";
import Card from "../layout/Card";
import CardContainer from "../layout/CardContainer";
import FooterHelper from "../layout/FooterHelper";
import HelperBox from "../layout/HelperBox";
import MainBox from "../layout/MainBox";
import numeral from "numeral";
import { useEffect, useRef, useState } from "react";

export function meta() {
  return [
    { title: "Kaspa Transactions List | Kaspa Explorer" },
    {
      name: "description",
      content:
        "Track the latest Kaspa transactions. View transaction ID, sender, recipient, fees, and block confirmations.",
    },
    { name: "keywords", content: "Kaspa transactions, blockchain transfers, transaction ID, sender, receiver, fees" },
  ];
}

export default function Transactions() {
  const { blocks, transactions, avgTxRate } = useIncomingBlocks();
  const { data: feeEstimate, isLoading: isLoadingFee } = useFeeEstimate();
  const { mempoolSize: mempoolSize } = useMempoolSize();
  const [avgTpsSinceOpen, setAvgTpsSinceOpen] = useState(0);
  const [totalTxSinceOpen, setTotalTxSinceOpen] = useState(0);
  const statsStartedAtRef = useRef<number | null>(null);
  const seenBlockHashesRef = useRef<Set<string>>(new Set());
  const txCountSinceOpenRef = useRef(0);

  useEffect(() => {
    if (!blocks.length) return;

    const now = Date.now();
    if (statsStartedAtRef.current === null) {
      statsStartedAtRef.current = now;
      seenBlockHashesRef.current = new Set(blocks.map((block) => block.block_hash));
      txCountSinceOpenRef.current = 0;
      setTotalTxSinceOpen(0);
      setAvgTpsSinceOpen(0);
      return;
    }

    let newTxs = 0;
    for (const block of blocks) {
      if (!seenBlockHashesRef.current.has(block.block_hash)) {
        seenBlockHashesRef.current.add(block.block_hash);
        newTxs += block.txCount || 0;
      }
    }

    if (newTxs > 0) {
      txCountSinceOpenRef.current += newTxs;
      setTotalTxSinceOpen(txCountSinceOpenRef.current);
      const elapsedSeconds = Math.max((now - statsStartedAtRef.current) / 1000, 1);
      setAvgTpsSinceOpen(txCountSinceOpenRef.current / elapsedSeconds);
    }
  }, [blocks]);

  const regularFee =
    feeEstimate && feeEstimate.normalBuckets && feeEstimate.normalBuckets.length > 0
      ? (feeEstimate.normalBuckets[0].feerate * 2036) / 1_0000_0000
      : 0;

  return (
    <>
      <MainBox>
        <CardContainer title="Transactions">
          <Card title="Total transactions" value={numeral(totalTxSinceOpen).format("0,0")} />
          <Card title="Average TPS" value={`${numeral(avgTpsSinceOpen || avgTxRate).format("0.0")}`} />
          <Card
            title="Regular fee"
            value={`${numeral(regularFee).format("0.00000000")} KAS`}
            loading={isLoadingFee}
          />
          <Card title="Mempool size" value={mempoolSize} />
        </CardContainer>
      </MainBox>

      <MainBox>
        <HelperBox>Blocks and its transactions are arriving with a speed of 10 blocks per second.</HelperBox>

        <PageTable
          className="text-black w-full"
          headers={["Timestamp", "Transaction ID", "Amount"]}
          additionalClassNames={{ 1: "overflow-hidden " }}
          rows={transactions.map((transaction) => [
            "a moment ago",
            <KasLink linkType="transaction" link to={transaction.txId} mono />,
            <>
              {numeral(transaction.outputs.reduce((acc, output) => acc + Number(output[1]), 0) / 1_0000_0000).format(
                "0,0.[00]",
              )}
              <span className="text-gray-500 text-nowrap"> KAS</span>
            </>,
          ])}
        />
      </MainBox>
      <FooterHelper icon={Transaction}>
        A transaction is a cryptographically signed command that modifies the blockchain's state. Block explorers
        monitor and display the details of every transaction within the network.
      </FooterHelper>
    </>
  );
}
