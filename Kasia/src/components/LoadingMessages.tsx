import { useState, useEffect } from "react";
import { LoaderCircle } from "lucide-react";

const LOADING_MESSAGES = [
  { delay: 0, message: "Starting the message client..." },
  { delay: 2000, message: "Loading your message history..." },
  {
    delay: 10000,
    message:
      "Still loading... \nFun Fact: Kaspa has processed 100 blocks since you started loading.",
  },
  { delay: 14000, message: "Still loading... \nActually.. 140..." },
  { delay: 18000, message: "Still loading... \nNow.. 180..." },
  {
    delay: 20000,
    message: "Yep, still loading... \nOk, now its too many blocks to count.",
  },
  {
    delay: 25000,
    message:
      "Yep, still loading... \nWe just didnt expect you to talk to so many people.\nDon't worry, we will fix this long wait soon.",
  },
];

export const LoadingMessages = () => {
  const [currentMessage, setCurrentMessage] = useState(
    LOADING_MESSAGES[0].message
  );

  useEffect(() => {
    const timeouts = LOADING_MESSAGES.slice(1).map(({ delay, message }) =>
      setTimeout(() => setCurrentMessage(message), delay)
    );

    return () => {
      timeouts.forEach(clearTimeout);
    };
  }, []);

  return (
    <div className="flex w-full flex-col items-center text-xs">
      <div className="border-primary-border bg-secondary-bg relative h-full w-full overflow-hidden border-t">
        <div className="bg-secondary-bg/20 absolute inset-0" />
        <div className="relative flex h-full flex-col items-center justify-center space-y-4 select-none">
          <span className="text-text-secondary text-center text-sm font-medium tracking-wide whitespace-pre-line sm:text-lg">
            {currentMessage}
          </span>
          <LoaderCircle className="text-text-secondary h-14 w-14 animate-spin" />
        </div>
      </div>
    </div>
  );
};
