import { Navigate, useParams } from "react-router";

export default function TxsIdRedirect() {
  const { id } = useParams();
  if (!id) return null;
  return <Navigate to={`/transactions/${id}`} replace />;
}
