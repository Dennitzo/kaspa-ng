export interface CompressImageOptions {
  minWidth?: number;
  minHeight?: number;
  maxQuality?: number;
  minQuality?: number;
  qualityStep?: number;
}
const DEFAULT_COMPRESSION_CONFIG: CompressImageOptions = {
  minWidth: 150,
  minHeight: 150,
  maxQuality: 0.95,
  minQuality: 0.3,
  qualityStep: 0.05,
};

export interface PrepareFileResult {
  fileMessage?: string;
  file?: File;
  error?: string;
}

const toDataURL = (blob: Blob): Promise<string> =>
  new Promise((res, rej) => {
    const r = new FileReader();
    r.onload = () =>
      typeof r.result === "string"
        ? res(r.result)
        : rej(new Error("Failed to read file as base64"));
    r.onerror = () => rej(r.error);
    r.readAsDataURL(blob);
  });

const JSON_WRAP_LEN = // we need this to gauge how much we space we need beyond the file itself
  JSON.stringify({ type: "file", name: "", size: 0, mimeType: "", content: "" })
    .length + 23;

// Helper: create file message and return its payload size
function createFilePayload(
  file: File,
  content: string
): { message: string; size: number } {
  const message = JSON.stringify({
    type: "file",
    name: file.name,
    size: file.size,
    mimeType: file.type,
    content,
  });
  return { message, size: new Blob([message]).size };
}

/**
 * prepare a file for upload: compresses if needed, encodes as base64, returns JSON message or error.
 */
export async function prepareFileForUpload(
  file: File,
  maxSize: number,
  compressOptions: CompressImageOptions = DEFAULT_COMPRESSION_CONFIG,
  onStatus?: (status: string) => void
): Promise<PrepareFileResult> {
  // Note: We now pass maxSize directly to compressImageToFit which validates actual payload size
  const rawTarget = Math.floor(((maxSize - JSON_WRAP_LEN) * 3) / 4);
  {
    /*default return for NON images */
  }
  if (file.size > rawTarget && !file.type.startsWith("image/"))
    return {
      error: `File too large. Please keep files under ${maxSize / 1024 - 5}KB to ensure it fits in a Kaspa transaction.`,
    };

  let candidate: File | null = file;
  let wasCompressed = false;
  if (file.size > rawTarget) {
    onStatus?.("Large Image - Compressing...");
    // Pass maxSize instead of rawTarget to validate actual JSON payload size
    candidate = await compressImageToFit(file, maxSize, compressOptions);
    wasCompressed = !!candidate && candidate !== file;
  }

  if (!candidate)
    return {
      error: `Image is too large and could not be compressed under ${maxSize / 1024}KB.`,
    };

  try {
    const content = await toDataURL(candidate);
    const { message: fileMessage, size: byteLen } = createFilePayload(
      candidate,
      content
    );
    if (byteLen > maxSize) {
      return {
        error:
          "Encoded file data too large for a Kaspa transaction. Please use a smaller file.",
      };
    } else {
      if (wasCompressed) {
        onStatus?.("Compression Successful");
      }
      return { fileMessage, file: candidate };
    }
  } catch (e) {
    return {
      error: `Failed to read file: ${e instanceof Error ? e.message : "Unknown error"}`,
    };
  }
}

// Helper to calculate actual JSON payload size for a file
async function getActualPayloadSize(file: File): Promise<number> {
  const content = await toDataURL(file);
  return createFilePayload(file, content).size;
}

// Calculate target dimensions maintaining aspect ratio
function calculateTargetDimensions(
  targetMaxDimension: number,
  originalWidth: number,
  originalHeight: number,
  minDimension: number
): { width: number; height: number } {
  const aspectRatio = originalWidth / originalHeight;
  const clampedMax = Math.max(minDimension, targetMaxDimension);

  if (aspectRatio >= 1) {
    const width = Math.min(clampedMax, originalWidth);
    const height = Math.max(minDimension, Math.round(width / aspectRatio));
    return { width, height };
  }

  const height = Math.min(clampedMax, originalHeight);
  const width = Math.max(minDimension, Math.round(height * aspectRatio));
  return { width, height };
}

// Generate quality levels from min to max
function generateQualityLevels(
  minQuality: number,
  maxQuality: number,
  qualityStep: number
): number[] {
  const qualities: number[] = [];
  for (let q = maxQuality; q >= minQuality; q -= qualityStep) {
    qualities.push(Math.round(q * 100) / 100);
  }
  if (qualities[qualities.length - 1] !== minQuality) {
    qualities.push(minQuality);
  }
  return qualities.reverse(); // Start from minimum quality
}

// New compression logic: preserve original size, only shrink when necessary
async function compressImageToFit(
  file: File,
  rawTarget: number,
  options: CompressImageOptions
): Promise<File | null> {
  const {
    minWidth = DEFAULT_COMPRESSION_CONFIG.minWidth!,
    minHeight = DEFAULT_COMPRESSION_CONFIG.minHeight!,
    maxQuality = DEFAULT_COMPRESSION_CONFIG.maxQuality!,
    minQuality = DEFAULT_COMPRESSION_CONFIG.minQuality!,
    qualityStep = DEFAULT_COMPRESSION_CONFIG.qualityStep!,
  } = options;

  const minDimension = Math.max(minWidth, minHeight);

  // Load original image
  const img = await new Promise<HTMLImageElement>((res, rej) => {
    const image = new Image();
    image.onload = () => res(image);
    image.onerror = rej;
    image.src = URL.createObjectURL(file);
  });

  const originalWidth = img.width;
  const originalHeight = img.height;
  const maxOriginalDimension = Math.max(originalWidth, originalHeight);

  const canvas = document.createElement("canvas");
  const ctx = canvas.getContext("2d");
  if (!ctx) return null;

  // Step 1: Binary search to find maximum dimension that fits at minimum quality
  const DIMENSION_THRESHOLD = 25; // Stop when range is smaller than this
  console.log(
    `[compressImageToFit] Step 1: Binary search for max dimension at quality ${minQuality}, maxPayload=${rawTarget} bytes`
  );

  let lowerBound = minDimension;
  let upperBound = Math.round(maxOriginalDimension);
  let chosenDimension: number | null = null;
  let bestDataSize = 0;

  // First check if even the minimum dimension fits
  const minTarget = calculateTargetDimensions(
    minDimension,
    originalWidth,
    originalHeight,
    minDimension
  );
  canvas.width = minTarget.width;
  canvas.height = minTarget.height;
  ctx.drawImage(img, 0, 0, minTarget.width, minTarget.height);
  const minCompressed = await tryCompress(canvas, file, minQuality, rawTarget);

  if (!minCompressed) {
    console.log(
      `[compressImageToFit] ✗ Even minimum dimension ${minDimension}px (${minTarget.width}x${minTarget.height}) is too large`
    );
    return null;
  }

  const minPayloadSize = await getActualPayloadSize(minCompressed);
  console.log(
    `[compressImageToFit] ✓ Minimum dimension ${minDimension}px (${minTarget.width}x${minTarget.height}) | Quality: ${minQuality} | Payload: ${minPayloadSize} bytes`
  );
  chosenDimension = minDimension;
  bestDataSize = minPayloadSize;

  // Binary search for optimal dimension
  while (upperBound - lowerBound > DIMENSION_THRESHOLD) {
    const midDimension = Math.round((lowerBound + upperBound) / 2);

    const target = calculateTargetDimensions(
      midDimension,
      originalWidth,
      originalHeight,
      minDimension
    );

    canvas.width = target.width;
    canvas.height = target.height;
    ctx.drawImage(img, 0, 0, target.width, target.height);

    const compressed = await tryCompress(canvas, file, minQuality, rawTarget);

    if (compressed) {
      const payloadSize = await getActualPayloadSize(compressed);
      console.log(
        `[compressImageToFit] ✓ Dimension ${midDimension}px (${target.width}x${target.height}) | Quality: ${minQuality} | Payload: ${payloadSize} bytes | Range: [${lowerBound}, ${upperBound}]`
      );
      chosenDimension = midDimension;
      bestDataSize = payloadSize;
      lowerBound = midDimension; // Try larger
    } else {
      console.log(
        `[compressImageToFit] ✗ Dimension ${midDimension}px (${target.width}x${target.height}) | Quality: ${minQuality} | Too large | Range: [${lowerBound}, ${upperBound}]`
      );
      upperBound = midDimension; // Try smaller
    }
  }

  if (!chosenDimension) {
    console.log(
      "[compressImageToFit] No dimension fits even at minimum quality"
    );
    return null;
  }

  console.log(
    `[compressImageToFit] Binary search complete: chosen dimension ${chosenDimension}px | Payload: ${bestDataSize} bytes`
  );

  // Step 2: For chosen dimension, find maximum quality that fits
  const qualities = generateQualityLevels(minQuality, maxQuality, qualityStep);
  let bestResult: File | null = null;

  const targetDims = calculateTargetDimensions(
    chosenDimension,
    originalWidth,
    originalHeight,
    minDimension
  );

  console.log(
    `[compressImageToFit] Step 2: Finding max quality for ${targetDims.width}x${targetDims.height}`
  );

  canvas.width = targetDims.width;
  canvas.height = targetDims.height;
  ctx.drawImage(img, 0, 0, targetDims.width, targetDims.height);

  let bestQuality = minQuality;
  for (const quality of qualities) {
    const compressed = await tryCompress(canvas, file, quality, rawTarget);
    if (!compressed) {
      if (bestResult) {
        console.log(
          `[compressImageToFit] ✗ Quality ${quality.toFixed(2)} | Too large`
        );
        break; // Previous quality was the max that fits
      }
      console.log(
        `[compressImageToFit] ✗ Quality ${quality.toFixed(2)} | Too large`
      );
      continue;
    }
    bestResult = compressed;
    bestQuality = quality;
    const payloadSize = await getActualPayloadSize(compressed);
    console.log(
      `[compressImageToFit] ✓ Quality ${quality.toFixed(2)} | Payload: ${payloadSize} bytes`
    );
  }

  if (bestResult) {
    const finalPayloadSize = await getActualPayloadSize(bestResult);
    console.log(
      `[compressImageToFit] Final result: ${targetDims.width}x${targetDims.height} @ quality ${bestQuality.toFixed(2)} | Payload: ${finalPayloadSize} bytes`
    );
  }

  return bestResult;
}

// Compresses once with given params and validates against actual payload size
async function tryCompress(
  canvas: HTMLCanvasElement,
  original: File,
  quality: number,
  maxPayloadSize: number
): Promise<File | null> {
  for (const type of ["image/webp", "image/jpeg"]) {
    const blob: Blob | null = await new Promise((res) =>
      canvas.toBlob(res, type, quality)
    );

    if (!blob) continue;

    // Create file and check actual JSON-wrapped base64 size (same as final validation)
    const file = new File(
      [blob],
      original.name.replace(/\.[^.]+$/, `.${type.split("/")[1]}`),
      { type }
    );

    // Convert to base64 and wrap in JSON like prepareFileForUpload does
    const content = await toDataURL(file);
    const actualPayloadSize = createFilePayload(file, content).size;

    if (actualPayloadSize <= maxPayloadSize) {
      return file;
    }
  }
  return null;
}
