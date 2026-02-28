import EventEmitter from "eventemitter3";
import {
  Address,
  addressFromScriptPublicKey,
  IVirtualChainChanged,
  RpcClient,
} from "kaspa-wasm";
import { VirtualChainEvent } from "../types/all";
import { TransactionId } from "../types/transactions";
import { devMode } from "../config/dev-mode";

export class ResolutionTimeoutException extends Error {}

export type ResolvedKasiaTransaction = {
  acceptingBlock: string;
  txId: string;
  sender: Address;
};

export type PendingTransactionResolutionContext = {
  resolve: (
    value: ResolvedKasiaTransaction | PromiseLike<ResolvedKasiaTransaction>
  ) => void;
  reject: (reason?: Error) => void;
  isBeingResolved: boolean;
};

export interface LiveServiceEvents {
  vccMutated: () => void;
}

/**
 * Responsability: Follow VCC for the last GARBAGE_COLLECTION_TIMEOUT_MS
 * Store acceptance data during this period
 *
 * Able to resolve a sender for a transaction id if this transaction has been seen during the GARBAGE_COLLECTION_TIMEOUT_MS time window
 */
export class SenderAndAcceptanceResolutionService extends EventEmitter<LiveServiceEvents> {
  RESOLUTION_TIMEOUT_MS = 10_000;
  GARBAGE_COLLECTION_TIMEOUT_MS = 15_000;
  private networkId = "mainnet";

  private waitingQueue: Map<
    TransactionId,
    PendingTransactionResolutionContext
  > = new Map();

  private transactionIdsByAcceptingBlock: Record<string, Set<string>> = {};
  private acceptingBlockByTransactionId: Record<string, string> = {};

  // https://262.ecma-international.org/6.0/#sec-ordinary-object-internal-methods-and-internal-slots-ownpropertykeys
  // iteration is ordered ascending as per the spec
  // warning: can contains duplicate elements, for two timestamp same block id can appear
  // it's safe in our case because we only use it as a garbage collection marker
  private acceptingBlocksByFirstSeen: Record<number, Set<string>> = {};

  private boundedProcessWaitingQueue = this.processWaitingQueue.bind(this);
  private boundedOnVirtualChainChanged = this.onVirtualChainChanged.bind(this);

  constructor(private readonly rpcClient: RpcClient) {
    super();

    this.init();
  }

  async init() {
    this.addListener("vccMutated", this.boundedProcessWaitingQueue);

    this.rpcClient.addEventListener(
      "virtual-chain-changed",
      this.boundedOnVirtualChainChanged
    );
    await this.rpcClient.subscribeVirtualChainChanged(true);
    try {
      const serverInfo = await this.rpcClient.getServerInfo();
      if (serverInfo?.networkId) {
        this.networkId = serverInfo.networkId;
      }
    } catch (error) {
      if (devMode) {
        console.warn("Unable to resolve networkId from RPC server info:", error);
      }
    }

    // if (devMode) {
    //   setInterval(() => {
    //     console.log("VCC Internal Tracking", {
    //       transactionLength: Object.keys(this.acceptingBlockByTransactionId)
    //         .length,
    //       acceptingBlockLength: Object.keys(
    //         this.transactionIdsByAcceptingBlock
    //       ),
    //       markedAsTouchedLength: Object.keys(this.acceptingBlocksByFirstSeen),
    //     });
    //   }, 2000);
    // }
  }

  async stop() {
    this.rpcClient.removeEventListener(
      "virtual-chain-changed",
      this.boundedOnVirtualChainChanged
    );
    this.removeListener("vccMutated", this.boundedProcessWaitingQueue);

    this.waitingQueue.clear();
    this.acceptingBlockByTransactionId = {};
    this.acceptingBlocksByFirstSeen = {};
    this.transactionIdsByAcceptingBlock = {};
  }

  async askResolution(txId: string): Promise<ResolvedKasiaTransaction> {
    const resolutionPromise = new Promise<ResolvedKasiaTransaction>(
      (res, rej) => {
        this.waitingQueue.set(txId, {
          reject: rej,
          resolve: res,
          isBeingResolved: false,
        });
      }
    );

    const raceResult = await Promise.race<
      [Promise<"timeout">, Promise<ResolvedKasiaTransaction>]
    >([
      new Promise((res) =>
        setTimeout(() => res("timeout"), this.RESOLUTION_TIMEOUT_MS)
      ),
      resolutionPromise,
    ]);

    this.waitingQueue.delete(txId);

    if (typeof raceResult === "string") {
      throw new ResolutionTimeoutException();
    }

    return raceResult;
  }

  private async processWaitingQueue() {
    if (this.waitingQueue.size > 0) {
      for (const [txId, waitingContext] of this.waitingQueue) {
        if (waitingContext.isBeingResolved) {
          // skip this iteration
          continue;
        }

        const acceptingBlock = this.acceptingBlockByTransactionId[txId];
        console.log(
          `resolving ${txId}, with accepting block ${acceptingBlock}`
        );
        if (acceptingBlock) {
          // mark as being processed
          this.waitingQueue.set(txId, {
            ...waitingContext,
            isBeingResolved: true,
          });

          try {
            const sender = await this.resolveSenderFromAcceptingBlock(
              txId,
              acceptingBlock
            );

            if (sender === null && devMode) {
              console.warn(
                `resolving ${txId}, this try failed, trying again...`,
                {
                  acceptingBlock,
                }
              );
              continue;
            }

            // release if still existing
            if (this.waitingQueue.has(txId)) {
              this.waitingQueue.set(txId, {
                ...waitingContext,
                isBeingResolved: false,
              });
            }

            if (sender) {
              waitingContext.resolve({
                acceptingBlock: acceptingBlock,
                txId,
                sender,
              });
            }
          } catch (error) {
            console.error("vcc waiting queue process error:", error);
            // release if still existing
            if (this.waitingQueue.has(txId)) {
              this.waitingQueue.set(txId, {
                ...waitingContext,
                isBeingResolved: false,
              });
            }
          }
        }
      }
    }
  }

  private async resolveSenderFromAcceptingBlock(
    txId: string,
    acceptingBlock: string
  ): Promise<Address | null> {
    const blockResponse = await this.rpcClient.getBlock({
      hash: acceptingBlock,
      includeTransactions: true,
    });
    const transactions = blockResponse?.block?.transactions ?? [];

    const tx = transactions.find((candidate) => {
      const candidateId = candidate.verboseData?.transactionId;
      const candidateHash = candidate.verboseData?.hash;
      return candidateId === txId || candidateHash === txId;
    });

    if (!tx) {
      return null;
    }

    for (const input of tx.inputs ?? []) {
      const address = input.utxo?.address;
      if (address) {
        return address;
      }

      if (input.utxo?.scriptPublicKey) {
        try {
          const derived = addressFromScriptPublicKey(
            input.utxo.scriptPublicKey,
            this.networkId
          );
          if (derived) {
            return derived;
          }
        } catch {
          // ignore and keep trying other inputs
        }
      }
    }

    return null;
  }

  private onVirtualChainChanged(_event: IVirtualChainChanged) {
    const data = _event.data as unknown as VirtualChainEvent["data"];
    const now = Date.now();

    const upperBoundAsString = String(now - this.GARBAGE_COLLECTION_TIMEOUT_MS);
    const acceptingBlockIdsToGarbage: string[] = [];

    // get hashes to invalidate: garbaged collected elements
    for (const timestampAsString in this.acceptingBlocksByFirstSeen) {
      if (timestampAsString >= upperBoundAsString) {
        break;
      }
      // mark to garbage
      acceptingBlockIdsToGarbage.push(
        ...this.acceptingBlocksByFirstSeen[timestampAsString]
      );

      // remove from timestamped
      delete this.acceptingBlocksByFirstSeen[timestampAsString];
    }

    // also add removed blockhash
    acceptingBlockIdsToGarbage.push(...data.removedChainBlockHashes);

    // garbage routine
    for (const acceptingBlockIdToGarbage of acceptingBlockIdsToGarbage) {
      const txIdsToRemove =
        this.transactionIdsByAcceptingBlock[
          acceptingBlockIdToGarbage
        ]?.values() ?? [];

      delete this.transactionIdsByAcceptingBlock[acceptingBlockIdToGarbage];

      for (const txIdToRemove of txIdsToRemove) {
        delete this.acceptingBlockByTransactionId[txIdToRemove];
      }
    }

    for (const acceptedTransactionId of data.acceptedTransactionIds) {
      // update in-memory
      const currentTxIdsByAccBlock =
        this.transactionIdsByAcceptingBlock[
          acceptedTransactionId.acceptingBlockHash
        ];
      if (!currentTxIdsByAccBlock) {
        this.transactionIdsByAcceptingBlock[
          acceptedTransactionId.acceptingBlockHash
        ] = new Set(acceptedTransactionId.acceptedTransactionIds);
      } else {
        this.transactionIdsByAcceptingBlock[
          acceptedTransactionId.acceptingBlockHash
        ] = new Set([
          ...currentTxIdsByAccBlock,
          ...acceptedTransactionId.acceptedTransactionIds,
        ]);
      }

      // set as touched
      const currentAcceptingBlocksByFirstSeen =
        this.acceptingBlocksByFirstSeen[now];
      if (!currentAcceptingBlocksByFirstSeen) {
        this.acceptingBlocksByFirstSeen[now] = new Set([
          acceptedTransactionId.acceptingBlockHash,
        ]);
      } else {
        this.acceptingBlocksByFirstSeen[now] = new Set([
          ...currentAcceptingBlocksByFirstSeen,
          acceptedTransactionId.acceptingBlockHash,
        ]);
      }

      // update it's reverse counterpart (txId -> acceptedBlock)
      for (const transactionId of acceptedTransactionId.acceptedTransactionIds) {
        this.acceptingBlockByTransactionId[transactionId] =
          acceptedTransactionId.acceptingBlockHash;
      }
    }

    if (data.acceptedTransactionIds.length > 0) {
      // emit vcc mutated
      this.emit("vccMutated");
    }
  }
}
