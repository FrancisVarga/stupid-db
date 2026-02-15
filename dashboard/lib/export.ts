/**
 * Export utilities for CSV, SVG, and PNG downloads.
 */

function downloadBlob(
  content: string | Blob,
  filename: string,
  mimeType: string
) {
  const blob =
    content instanceof Blob
      ? content
      : new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

/**
 * Export an array of objects as a CSV file.
 */
export function exportCSV(
  data: Record<string, string | number>[],
  filename: string
) {
  if (data.length === 0) return;

  const columns = Object.keys(data[0]);
  const header = columns.map(escapeCSV).join(",");
  const rows = data.map((row) =>
    columns.map((col) => escapeCSV(String(row[col] ?? ""))).join(",")
  );

  const csv = [header, ...rows].join("\n");
  downloadBlob(csv, `${filename}.csv`, "text/csv");
}

function escapeCSV(value: string): string {
  if (value.includes(",") || value.includes('"') || value.includes("\n")) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

/**
 * Export an SVG element as an SVG file.
 */
export function exportSVG(svgElement: SVGElement, filename: string) {
  const serializer = new XMLSerializer();
  const svgData = serializer.serializeToString(svgElement);
  const svgWithHeader = `<?xml version="1.0" encoding="UTF-8"?>\n${svgData}`;
  downloadBlob(svgWithHeader, `${filename}.svg`, "image/svg+xml");
}

/**
 * Export an SVG element as a PNG file.
 */
export function exportPNG(svgElement: SVGElement, filename: string) {
  const serializer = new XMLSerializer();
  const svgData = serializer.serializeToString(svgElement);
  const svgBlob = new Blob([svgData], {
    type: "image/svg+xml;charset=utf-8",
  });
  const url = URL.createObjectURL(svgBlob);

  const img = new Image();
  img.onload = () => {
    const canvas = document.createElement("canvas");
    const scale = 2; // 2x for retina
    canvas.width = svgElement.clientWidth * scale;
    canvas.height = svgElement.clientHeight * scale;
    const ctx = canvas.getContext("2d")!;
    ctx.scale(scale, scale);
    // Dark background
    ctx.fillStyle = "#06080d";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.drawImage(img, 0, 0);
    canvas.toBlob((blob) => {
      if (blob) {
        downloadBlob(blob, `${filename}.png`, "image/png");
      }
      URL.revokeObjectURL(url);
    }, "image/png");
  };
  img.src = url;
}
