import {
  getContextualMessagesBySender,
  getHandshakesByReceiver,
  getPaymentsBySender,
  getSelfStashByOwner,
  HandshakeResponse,
  SelfStashResponse,
} from "./indexer/generated";
import { isIndexerDisabled } from "../utils/indexer-settings";

/**
 * Handles historical events
 */
export class HistoricalSyncer {
  private get DISABLED(): boolean {
    return isIndexerDisabled();
  }

  constructor(readonly address: string) {}

  /**
   * Emits `initialLoadCompleted` when done
   */
  async initialLoad(
    lastSavedHandshakeTimestamp: number,
    lastHandshakeTimestamp: number
  ): Promise<{
    receivedHandshakes: ({ __type: "received" } & HandshakeResponse)[];
    sentHandshakes: ({ __type: "sent" } & SelfStashResponse)[];
  }> {
    if (this.DISABLED) {
      console.log("HistoricalSyncer: initialLoad disabled");
      return {
        receivedHandshakes: [],
        sentHandshakes: [],
      };
    }

    // fetch all historical handhskaes for this address
    const [receivedHandshakes, sentHanshakes] = await Promise.all([
      getHandshakesByReceiver({
        query: {
          address: this.address,
          block_time: BigInt(lastHandshakeTimestamp),
          limit: 100,
        },
      }),
      this.fetchHistoricalSelfStashByOwner(
        this.address,
        "saved_handshake",
        lastSavedHandshakeTimestamp
      ),
    ]);

    if (receivedHandshakes.error) {
      console.error(
        "Error fetching received handshakes",
        receivedHandshakes.error
      );
    }

    return {
      receivedHandshakes: receivedHandshakes.data
        ? receivedHandshakes.data.map((e) => ({ ...e, __type: "received" }))
        : [],
      sentHandshakes: sentHanshakes.map((e) => ({ ...e, __type: "sent" })),
    };
  }

  async fetchHistoricalMessagesToAddress(
    from: string,
    alias: string,
    after?: number
  ) {
    if (this.DISABLED) {
      console.log(
        "HistoricalSyncer: fetchHistoricalMessagesToAddress disabled"
      );
      return [];
    }

    const messages = await getContextualMessagesBySender({
      query: {
        address: from,
        // encode alias as hex string using text encoder
        alias: new TextEncoder()
          .encode(alias)
          .reduce((acc, byte) => acc + byte.toString(16).padStart(2, "0"), ""),
        limit: 100,
        block_time: after ? BigInt(after) : undefined,
      },
    });

    if (messages.error) {
      console.error("Error fetching messages", messages.error);
      return [];
    }

    return messages.data;
  }

  async fetchHistoricalPaymentsFromAddress(from: string, after?: number) {
    if (this.DISABLED) {
      console.log(
        "HistoricalSyncer: fetchHistoricalPaymentsFromAddress disabled"
      );
      return [];
    }

    const payments = await getPaymentsBySender({
      query: {
        address: from,
        limit: 100,
        block_time: after ? BigInt(after) : undefined,
      },
    });

    if (payments.error) {
      console.error("Error fetching payments", payments.error);
      return [];
    }

    return payments.data;
  }

  /**
   * @param owner owner kaspa address
   * @param scope of the stashed zone
   */
  async fetchHistoricalSelfStashByOwner(
    owner: string,
    scope: string,
    after?: number
  ) {
    if (this.DISABLED) {
      console.log("HistoricalSyncer: fetchHistoricalSelfStashByOwner disabled");
      return [];
    }

    const stashedElements = await getSelfStashByOwner({
      query: {
        owner,
        // encode alias as hex string using text encoder
        scope: new TextEncoder()
          .encode(scope)
          .reduce((acc, byte) => acc + byte.toString(16).padStart(2, "0"), ""),
        block_time: after ? BigInt(after) : undefined,
        limit: 100,
      },
    });

    if (stashedElements.error) {
      console.error("Error fetching self stash", stashedElements.error);
      return [];
    }

    return stashedElements.data;
  }
}
