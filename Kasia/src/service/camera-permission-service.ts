import { FeatureFlags, useFeatureFlagsStore } from "../store/featureflag.store";
import { toast } from "../utils/toast-helper";

class CameraPermissionService {
  private static instance: CameraPermissionService;
  private readonly STORAGE_KEY = "camera_permission_granted";
  private lastFeatureState: boolean = false;
  private constructor() {}

  static getInstance(): CameraPermissionService {
    if (!CameraPermissionService.instance) {
      CameraPermissionService.instance = new CameraPermissionService();
    }
    return CameraPermissionService.instance;
  }

  // check if camera feature is enabled
  isFeatureEnabled(): boolean {
    try {
      const { flags } = useFeatureFlagsStore.getState();
      return flags[FeatureFlags.ENABLED_CAMERA] || false;
    } catch {
      return false;
    }
  }

  // check if we already have permission
  hasPermission(): boolean {
    try {
      return localStorage.getItem(this.STORAGE_KEY) === "true";
    } catch {
      return false;
    }
  }

  // clear permission when feature is disabled
  clearPermission(): void {
    try {
      localStorage.removeItem(this.STORAGE_KEY);
    } catch {
      console.log("localStorage not available");
    }
  }

  // main camera request method
  async requestCamera(): Promise<boolean> {
    // check if feature is enabled
    if (!this.isFeatureEnabled()) {
      toast.error("Enable Camera in Settings > Extras");
      return false;
    }

    // check browser support
    if (
      typeof navigator === "undefined" ||
      !navigator.mediaDevices?.getUserMedia
    ) {
      toast.error("Camera is not supported on this device");
      return false;
    }

    // if we already have permission, proceed
    if (this.hasPermission()) {
      return true;
    }

    try {
      // request camera access
      const stream = await navigator.mediaDevices.getUserMedia({
        video: { facingMode: { ideal: "environment" } },
      });
      stream.getTracks().forEach((t) => t.stop());

      // store permission granted
      try {
        localStorage.setItem(this.STORAGE_KEY, "true");
      } catch {
        console.log("localStorage not available");
      }

      return true;
    } catch (error) {
      // handle different error types
      const errorName = (error as DOMException)?.name;

      if (errorName === "NotFoundError") {
        toast.error("No camera available on this device");
        return false;
      }

      if (errorName === "NotAllowedError") {
        toast.error("Camera permission denied. Please allow camera access.");
        return false;
      }

      // other errors
      toast.error("Camera access failed. Please try again.");
      return false;
    }
  }
}

export const cameraPermissionService = CameraPermissionService.getInstance();
