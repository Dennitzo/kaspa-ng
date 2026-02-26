/**
 * Convert a data URL to a Blob
 */
function dataUrlToBlob(dataUrl: string): Blob {
  const parts = dataUrl.split(",");
  const mime = parts[0].match(/:(.*?);/)?.[1] || "";
  const bstr = atob(parts[1]);
  const n = bstr.length;
  const u8arr = new Uint8Array(n);
  for (let i = 0; i < n; i++) {
    u8arr[i] = bstr.charCodeAt(i);
  }
  return new Blob([u8arr], { type: mime });
}

/**
 * Safely trigger a file download that works across browsers including iOS Safari.
 *
 * Uses blob URLs instead of data URLs directly to avoid Safari tab navigation issues.
 * Creates an object URL from the blob, triggers download, then cleans up the URL.
 */
export function downloadFile(dataUrl: string, fileName: string): void {
  // Convert data URL to blob and create object URL
  const blob = dataUrlToBlob(dataUrl);
  const url = URL.createObjectURL(blob);

  const link = document.createElement("a");
  link.href = url;
  link.download = fileName;

  // Trigger download
  link.click();

  // Clean up object URL to free memory
  setTimeout(() => {
    URL.revokeObjectURL(url);
  }, 100);
}

/**
 * Extract file extension from a data URL
 * Generally format will be webp as thats what we compress to in `upload-file-service`
 */
function getExtensionFromDataUrl(dataUrl: string): string {
  if (dataUrl.startsWith("data:image/webp")) return "webp";
  if (dataUrl.startsWith("data:image/png")) return "png";
  if (
    dataUrl.startsWith("data:image/jpeg") ||
    dataUrl.startsWith("data:image/jpg")
  )
    return "jpg";
  if (dataUrl.startsWith("data:image/gif")) return "gif";
  return "png"; // default
}

/**
 * Generate a timestamped filename for an image
 */
function generateImageFilename(extension: string): string {
  const timestamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, -5);
  return `kasia-image-${timestamp}.${extension}`;
}

/**
 * Download an image from a data URL with an automatically generated filename
 */
export function downloadImage(dataUrl: string): void {
  const extension = getExtensionFromDataUrl(dataUrl);
  const fileName = generateImageFilename(extension);
  downloadFile(dataUrl, fileName);
}
