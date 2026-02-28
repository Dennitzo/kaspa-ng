import { socket, useSocketConnected } from "../api/socket";
import { useEffect } from "react";

interface UseSocketCommand<T> {
  command: string;
  onReceive?: (data: T) => void;
}

export const useSocketCommand = <T>({ command, onReceive }: UseSocketCommand<T>) => {
  const { connected, generation } = useSocketConnected();

  useEffect(() => {
    if (!connected || !command) return;
    const activeSocket = socket;

    activeSocket.emit(command, "");

    const handleResponse = (data: T) => {
      onReceive?.(data);
    };

    activeSocket.on(command, handleResponse);
    return () => {
      activeSocket.off(command, handleResponse);
    };
  }, [connected, generation, command, onReceive]);

  return {};
};
