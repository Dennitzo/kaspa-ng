import { socket, useSocketConnected } from "../api/socket";
import { useEffect } from "react";

const roomReferences: Record<string, number> = {};

interface UseSocketRoomOptions<T> {
  room: string;
  eventName: string;
  onMessage: (message: T) => void;
}

export const useSocketRoom = <T>({ room, onMessage, eventName }: UseSocketRoomOptions<T>) => {
  const { connected } = useSocketConnected();

  useEffect(() => {
    const handleConnect = () => {
      socket.emit("join-room", room);
    };

    roomReferences[room] = (roomReferences[room] || 0) + 1;
    socket.on("connect", handleConnect);
    if (socket.connected || connected) {
      handleConnect();
    }
    socket.on(eventName, onMessage);

    return () => {
      socket.off(eventName, onMessage);
      socket.off("connect", handleConnect);

      roomReferences[room]--;
      if (roomReferences[room] === 0) {
        // nothing to do now..
      }
    };
  }, [room, onMessage, connected]);

  return {};
};
