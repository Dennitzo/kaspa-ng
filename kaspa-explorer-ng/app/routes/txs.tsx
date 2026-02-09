import { Navigate } from "react-router";

export default function TxsRedirect() {
  return <Navigate to="/transactions" replace />;
}
