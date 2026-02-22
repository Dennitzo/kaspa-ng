import { index, route, type RouteConfig } from "@react-router/dev/routes";

export default [
  index("routes/home.tsx"),
  route("blocks", "./routes/blocks.tsx"),
  route("blocks/:blockId", "./routes/blockdetails.tsx"),
  route("transactions", "./routes/transactions.tsx"),
  route("mempool", "./routes/mempool.tsx"),
  route("transactions/:transactionId", "./routes/transactiondetails.tsx"),
  route("addresses/:address", "./routes/addressdetails.tsx"),
  route("miners", "./routes/miners.tsx"),

  route("donate", "./routes/donate.tsx"),
  route("analytics", "./routes/analytics.tsx"),

  route("*", "./routes/notfound.tsx"),

  // Add redirecting routes
  route("txs", "./routes/txs.tsx"),
  route("txs/:id", "./routes/txsdetails.tsx"),
] satisfies RouteConfig;
