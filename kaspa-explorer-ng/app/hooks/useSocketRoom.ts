import { socket, useSocketConnected } from "../api/socket";
import { useEffect } from "react";

const roomReferences: Record<string, number> = {};

interface UseSocketRoomOptions<T> {
  room: string;
  eventName: string;
  onMessage: (message: T) => void;
}

export const useSocketRoom = <T>({ room, onMessage, eventName }: UseSocketRoomOptions<T>) => {
  const { connected, generation } = useSocketConnected();

  useEffect(() => {
    const activeSocket = socket;
    const handleConnect = () => {
      activeSocket.emit("join-room", room);
    };

    roomReferences[room] = (roomReferences[room] || 0) + 1;
    activeSocket.on("connect", handleConnect);
    if (activeSocket.connected || connected) {
      handleConnect();
    }
    activeSocket.on(eventName, onMessage);

    return () => {
      activeSocket.off(eventName, onMessage);
      activeSocket.off("connect", handleConnect);

      roomReferences[room]--;
      if (roomReferences[room] === 0) {
        // nothing to do now..
      }
    };
  }, [room, onMessage, connected, generation, eventName]);

  return {};
};
