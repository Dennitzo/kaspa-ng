import React, { useRef, useEffect, useState } from "react";
import jsQR from "jsqr";
import { X } from "lucide-react";
import { toast } from "../../utils/toast-helper";
import { useUiStore } from "../../store/ui.store";
import { cameraPermissionService } from "../../service/camera-permission-service";

export const QrScannerModal: React.FC = () => {
  const videoRef = useRef<HTMLVideoElement>(null);
  const [stream, setStream] = useState<MediaStream | null>(null);

  const closeModal = useUiStore((s) => s.closeModal);
  const qrScannerCallback = useUiStore((s) => s.qrScannerCallback);
  const setQrScannerCallback = useUiStore((s) => s.setQrScannerCallback);

  // start camera when modal opens
  useEffect(() => {
    let localStream: MediaStream | null = null;
    let cancelled = false;
    let currentVideoElement: HTMLVideoElement | null = null;

    const waitForVideoAndStartCamera = async () => {
      let attempts = 0;
      while (!videoRef.current && attempts < 20) {
        await new Promise((r) => setTimeout(r, 50));
        attempts++;
      }
      if (!videoRef.current) {
        toast.error("Video element not able to start");
        closeModal("qr-scanner");
        return;
      }

      currentVideoElement = videoRef.current;

      // Use the simplified camera service
      const hasAccess = await cameraPermissionService.requestCamera();
      if (!hasAccess) {
        closeModal("qr-scanner");
        return;
      }

      try {
        // Get the camera stream (we already know permission is granted)
        localStream = await navigator.mediaDevices!.getUserMedia({
          video: { facingMode: { ideal: "environment" } },
          audio: false,
        });

        if (currentVideoElement && !cancelled) {
          const v = currentVideoElement;
          v.srcObject = localStream;
          v.setAttribute("playsinline", "true");
          v.muted = true;
          await new Promise<void>((res) => (v.onloadedmetadata = () => res())); // wait for dimensions
          await v.play();
          setStream(localStream);
        }
      } catch (e: unknown) {
        const msg =
          "Could not access camera: " +
          (e instanceof Error ? e.message : String(e));
        toast.error(msg);
        closeModal("qr-scanner");
      }
    };

    waitForVideoAndStartCamera();

    return () => {
      cancelled = true;
      if (localStream) {
        localStream.getTracks().forEach((track) => track.stop());
      }
      if (currentVideoElement) {
        currentVideoElement.srcObject = null;
      }
      setStream(null);
    };
  }, [closeModal]);

  // scan when stream is ready
  useEffect(() => {
    if (!stream) return;
    let id: number;
    // create an offscreen canvas for scanning
    const canvas = document.createElement("canvas");
    const tick = () => {
      const v = videoRef.current;
      if (v) {
        if (v.readyState < v.HAVE_ENOUGH_DATA) {
          v.play().catch(() => {});
        }
        if (v.readyState === v.HAVE_ENOUGH_DATA) {
          canvas.width = v.videoWidth;
          canvas.height = v.videoHeight;
          const ctx = canvas.getContext("2d", { willReadFrequently: true });
          if (ctx) {
            ctx.drawImage(v, 0, 0, canvas.width, canvas.height);
            const code = jsQR(
              ctx.getImageData(0, 0, canvas.width, canvas.height).data,
              canvas.width,
              canvas.height
            );
            if (code) {
              // Success feedback
              toast.success("QR Code Scanned");

              // Call the callback if it exists
              if (qrScannerCallback) {
                qrScannerCallback(code.data);
              }

              // Close modal and clear callback
              closeModal("qr-scanner");
              setQrScannerCallback(null);
              return;
            }
          }
        }
      }
      id = requestAnimationFrame(tick);
    };
    id = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(id);
  }, [stream, qrScannerCallback, closeModal, setQrScannerCallback]);

  return (
    <div className="fixed inset-0 z-[9999] bg-[var(--primary-bg)]/20">
      {/* Centered content container */}
      <div className="flex h-full items-center justify-center">
        {/* Camera view container with relative positioning for X button */}
        <div className="relative">
          {/* Close button - positioned on the image */}
          <button
            onClick={() => {
              closeModal("qr-scanner");
              setQrScannerCallback(null);
            }}
            className="hover:text-kas-secondary absolute top-2 right-2 z-10 cursor-pointer p-1 text-[var(--text-primary)] hover:scale-110 active:scale-90 active:opacity-80"
          >
            <X className="h-7 w-7 rounded-3xl" />
          </button>

          {/* Camera view - Always 80% of viewport height */}
          <video
            ref={videoRef}
            autoPlay
            muted
            playsInline
            className="h-[80vh] w-full max-w-md rounded-lg object-cover"
          />
        </div>

        {/* Scanning overlay */}
        <div className="absolute inset-0 flex flex-col items-center justify-center">
          {/* Scanning frame */}
          <div className="relative">
            <div className="h-64 w-64 rounded-lg border-2 border-[var(--primary-border)]/60"></div>
            {/* Scanning line animation */}
          </div>

          {/* Instructions */}
          <div className="mt-8 text-center">
            <div className="text-lg font-medium text-[var(--text-primary)]">
              Scan KAS QR
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
