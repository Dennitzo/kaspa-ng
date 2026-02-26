import { HeartHandshake } from "lucide-react";
import { useUiStore } from "../../store/ui.store";
import type { ModalType } from "../../store/ui.store";

interface DonationsProps {
  closeModalKey?: ModalType;
  position?: "top-left" | "top-right" | "bottom-left" | "bottom-right";
  onClick?: () => void;
}

export const Donations = ({
  closeModalKey,
  position = "top-left",
  onClick,
}: DonationsProps) => {
  const openModal = useUiStore((state) => state.openModal);
  const closeModal = useUiStore((state) => state.closeModal);

  const getPositionClasses = () => {
    const baseClasses = "fixed z-50";
    switch (position) {
      case "top-left":
        return `${baseClasses} top-2 left-2`;
      case "top-right":
        return `${baseClasses} top-2 right-2`;
      case "bottom-left":
        return `${baseClasses} bottom-2 left-2`;
      case "bottom-right":
        return `${baseClasses} bottom-2 right-2`;
      default:
        return `${baseClasses} top-2 left-2`;
    }
  };

  const handleClick = () => {
    if (onClick) {
      onClick();
    } else if (closeModalKey) {
      closeModal(closeModalKey);
    }
    openModal("donation");
  };

  return (
    <button
      onClick={handleClick}
      className={`shadow-4xl ${getPositionClasses()} cursor-pointer rounded-full p-2 text-pink-500/90 backdrop-blur-sm transition-colors duration-200 hover:text-pink-600`}
      title="Support us with a donation"
    >
      <HeartHandshake className="size-7" />
    </button>
  );
};
