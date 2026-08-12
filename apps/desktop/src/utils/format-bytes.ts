const UNITS = ["B", "KB", "MB", "GB", "TB", "PB"] as const;

/**
 * Formats a non-negative byte count using binary units while keeping the
 * displayed number compact enough for dashboard cards.
 *
 * @param bytes - Capacity in bytes. Non-finite and negative values are rejected.
 * @param maximumFractionDigits - Maximum number of decimal digits to render.
 * @returns A localized capacity string such as "40.43 GB".
 * @throws {RangeError} When bytes is negative, non-finite, or the precision is invalid.
 */
export function formatBytes(bytes: number, maximumFractionDigits = 2): string {
  if (!Number.isFinite(bytes) || bytes < 0) {
    throw new RangeError("bytes must be a finite non-negative number");
  }

  if (
    !Number.isInteger(maximumFractionDigits) ||
    maximumFractionDigits < 0 ||
    maximumFractionDigits > 3
  ) {
    throw new RangeError(
      "maximumFractionDigits must be an integer between 0 and 3",
    );
  }

  if (bytes === 0) {
    return "0 B";
  }

  const unitIndex = Math.min(
    Math.floor(Math.log(bytes) / Math.log(1024)),
    UNITS.length - 1,
  );
  const value = bytes / 1024 ** unitIndex;

  return (
    new Intl.NumberFormat("zh-CN", {
      maximumFractionDigits,
      minimumFractionDigits: 0,
    }).format(value) +
    " " +
    UNITS[unitIndex]
  );
}
