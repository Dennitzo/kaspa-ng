import { useUiStore } from "../../store/ui.store";
import { useEffect } from "react";
import { Download, X } from "lucide-react";
import { downloadImage } from "../../utils/file-download";

interface ImagePresenterProps {
  onClose: () => void;
}

// Image presenter modal - handles its own modal behavior
export const ImagePresenter = ({ onClose }: ImagePresenterProps) => {
  const { imagePresenterImage, setImagePresenterImage } = useUiStore();

  // clear image data when component unmounts (modal closes)
  useEffect(() => {
    return () => {
      setImagePresenterImage(null);
    };
  }, [setImagePresenterImage]);

  // escape key handling
  useEffect(() => {
    const handleEscape = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        onClose();
      }
    };

    document.addEventListener("keydown", handleEscape);
    return () => document.removeEventListener("keydown", handleEscape);
  }, [onClose]);

  if (!imagePresenterImage) {
    return null;
  }

  const handleDownload = () => {
    downloadImage(imagePresenterImage);
  };

  const handleClose = () => {
    onClose();
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div onClick={(e) => e.stopPropagation()}>
        <div className="relative inline-block shadow-2xl shadow-(color:--button-primary)/30">
          <img
            src={imagePresenterImage}
            alt="Presented image"
            className="h-auto max-h-[calc(100vh-3rem)] max-w-[calc(100vw-3rem)] rounded-lg object-contain"
          />
          <button
            onClick={handleDownload}
            className="absolute top-1 left-1 flex cursor-pointer items-center justify-center rounded-lg bg-[var(--button-primary)]/60 p-1.5 text-[var(--text-primary)] transition-colors hover:bg-[var(--button-primary)]/100 active:scale-95"
          >
            <Download className="size-5" />
            <span className="absolute p-6 pointer-fine:hidden" />
          </button>
          <button
            onClick={handleClose}
            className="hover:text-kas-secondary absolute top-0 right-0 z-60 cursor-pointer p-1 hover:scale-110 active:scale-90 active:opacity-80 sm:top-2 sm:right-2 sm:p-0"
          >
            <X className="h-7 w-7 rounded-3xl" />
          </button>
        </div>
      </div>
    </div>
  );
};
