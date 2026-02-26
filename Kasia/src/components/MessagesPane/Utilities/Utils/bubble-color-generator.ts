// color utils for consistent address-based colors

export const bubbleVar = "--bubble-color";

// HSL color generation configuration
export const colorConfig = {
  hueMultiplier: 137.508, // spreads seeds across color wheel
  saturation: {
    min: 40, // minimum saturation (50%)
    range: 20, // range (50 + 25 = max 74%)
  },
  lightness: {
    min: 45, // minimum lightness (45%)
    range: 35, // range (45 + 35 = max 79%)
  },
} as const;

const hslToHex = (h: number, s: number, l: number) => {
  s /= 100;
  l /= 100;
  const k = (n: number) => (n + h / 30) % 12;
  const a = s * Math.min(l, 1 - l);
  const f = (n: number) =>
    l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
  const rgb = [0, 8, 4].map((n) => Math.round(255 * f(n)));
  return "#" + rgb.map((v) => v.toString(16).padStart(2, "0")).join("");
};

// generate address colors for bubbles
export const generateAddressColor = (address: string) => {
  // use last 8 characters for better entropy (addresses end with hash-like strings)
  const cleanAddress = address.toLowerCase().replace(/^0x/, "");
  const last8 = cleanAddress.slice(-8); // get last 8 characters

  let seed = 0;
  for (let i = 0; i < last8.length; i++) {
    seed = (seed * 31 + last8.charCodeAt(i)) >>> 0;
  }

  const hue = (seed * colorConfig.hueMultiplier) % 360; // 0–360
  const sat =
    colorConfig.saturation.min + ((seed >>> 8) % colorConfig.saturation.range); // 50–74%
  const lig =
    colorConfig.lightness.min + ((seed >>> 16) % colorConfig.lightness.range); // 45–79%
  return hslToHex(hue, sat, lig); // solid color
};
