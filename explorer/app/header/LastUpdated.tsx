import { useIsFetching } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";

const formatDateTime = (date: Date) => {
  const datePart = date.toLocaleDateString("de-DE");
  const timePart = date.toLocaleTimeString("de-DE", {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
  return `${datePart}, ${timePart}`;
};

const LastUpdated = ({ className = "" }: { className?: string }) => {
  const isFetching = useIsFetching();
  const [lastUpdated, setLastUpdated] = useState(() => formatDateTime(new Date()));
  const prevFetching = useRef(isFetching);

  useEffect(() => {
    if (prevFetching.current > 0 && isFetching === 0) {
      setLastUpdated(formatDateTime(new Date()));
    }
    prevFetching.current = isFetching;
  }, [isFetching]);

  useEffect(() => {
    const intervalId = window.setInterval(() => {
      setLastUpdated(formatDateTime(new Date()));
    }, 60000);
    return () => window.clearInterval(intervalId);
  }, []);

  return (
    <div className={className}>
      LAST UPDATED: <span className="text-gray-500">{lastUpdated}</span>
    </div>
  );
};

export default LastUpdated;
