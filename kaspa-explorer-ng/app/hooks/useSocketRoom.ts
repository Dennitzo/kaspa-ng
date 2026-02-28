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
    const joinRoom = () => {
      activeSocket.emit("join-room", room);
    };
    const handleConnect = () => {
      joinRoom();
    };

    roomReferences[room] = (roomReferences[room] || 0) + 1;
    activeSocket.on("connect", handleConnect);
    if (activeSocket.connected || connected) {
      joinRoom();
    }

    // Some remote socket backends silently drop room memberships;
    // refresh room join periodically to keep streaming updates alive.
    const keepAliveIntervalId = setInterval(() => {
      if (activeSocket.connected) {
        joinRoom();
      }
    }, 20000);

    activeSocket.on(eventName, onMessage);

    return () => {
      activeSocket.off(eventName, onMessage);
      activeSocket.off("connect", handleConnect);
      clearInterval(keepAliveIntervalId);

      roomReferences[room]--;
      if (roomReferences[room] === 0) {
        // nothing to do now..
      }
    };
  }, [room, onMessage, connected, generation, eventName]);

  return {};
};
