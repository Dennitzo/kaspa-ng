import * as kaspaWasm from "kaspa-wasm";
import initCipherWasm from "cipher";
import "./utils/logging";
import { createRoot, type Root } from "react-dom/client";
import {
  mountSplashScreen,
  unmountSplashScreen,
} from "./components/Layout/Splash.ts";
import "./index.css";

let root: Root;
let splashElement: HTMLElement;

type KaspaInitModule = {
  default?: () => Promise<unknown>;
  init?: () => Promise<unknown>;
  initSync?: (...args: unknown[]) => unknown;
  initConsolePanicHook?: () => void;
};

async function initKaspaWasmCompat() {
  const mod = kaspaWasm as unknown as KaspaInitModule;

  if (typeof mod.default === "function") {
    await mod.default();
    return;
  }

  if (typeof mod.init === "function") {
    await mod.init();
    return;
  }

  throw new Error(
    "kaspa-wasm init function not found (expected default export or init())"
  );
}

// load wasm entry point, and lazy load sub-module so we don't have to worry
// about ordering of wasm module initialization
export async function boot() {
  const container = document.getElementById("root")!;

  // mount plain js splash screen
  splashElement = mountSplashScreen(container);

  await Promise.all([initKaspaWasmCompat(), initCipherWasm()]);

  const panicHook = (kaspaWasm as unknown as KaspaInitModule)
    .initConsolePanicHook;
  if (typeof panicHook === "function") {
    panicHook();
  }

  console.log("Kaspa SDK initialized successfully");

  root = createRoot(container);

  // lazy load main
  const { loadApplication } = await import("./main");
  await loadApplication(root);

  // lazy load network store and db store after the main app is running
  const [{ useDBStore }] = await Promise.all([import("./store/db.store")]);

  // init db if not initialized
  const { db, initDB } = useDBStore.getState();
  if (!db) initDB();
  // unmount splash screen after everything is loaded and ready
  unmountSplashScreen(splashElement);
}

boot();
