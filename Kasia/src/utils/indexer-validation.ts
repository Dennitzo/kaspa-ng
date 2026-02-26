export type IndexerStatus = "reachable" | "success" | "error";

const INDEXER_TEST_TIME = 1000;
// gets connection and proves processing with processed delta
export const checkIndexerHealth = async (
  url: string
): Promise<IndexerStatus> => {
  try {
    const firstResponse = await fetch(`${url}/metrics`);
    if (!firstResponse.ok) {
      return "error";
    }

    const firstMetrics = await firstResponse.json();

    if (!firstMetrics || !firstMetrics.blocks_processed) {
      return "error";
    }

    const firstBlocksProcessed = Number(firstMetrics.blocks_processed);

    await new Promise((resolve) => setTimeout(resolve, INDEXER_TEST_TIME));

    const secondResponse = await fetch(`${url}/metrics`);
    if (!secondResponse.ok) {
      return "error";
    }

    const secondMetrics = await secondResponse.json();

    if (!secondMetrics || !secondMetrics.blocks_processed) {
      return "error";
    }

    const secondBlocksProcessed = Number(secondMetrics.blocks_processed);

    return secondBlocksProcessed > firstBlocksProcessed
      ? "success"
      : "reachable";
  } catch (error) {
    console.error("Indexer validation failed:", error);
    return "error";
  }
};
