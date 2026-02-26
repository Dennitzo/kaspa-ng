import { vi } from "vitest";

vi.mock("kaspa-wasm", () => ({
  RpcClient: vi.fn(() => ({
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    subscribeVirtualChainChanged: vi.fn().mockResolvedValue(undefined),
    connect: vi.fn(),
    disconnect: vi.fn(),
  })),
  initConsolePanicHook: vi.fn(),
  default: vi.fn(),
}));

vi.mock("cipher", () => ({
  default: vi.fn(),
}));
