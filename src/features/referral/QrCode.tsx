import { useMemo } from "react";
import qrcode from "qrcode-generator";

interface Props {
  value: string;
  /** Rendered edge length in CSS pixels. */
  size?: number;
}

/**
 * QR code for a referral link, drawn as a single SVG path.
 *
 * The library's own `createSvgTag` hardcodes black-on-white; the widget is a dark terminal
 * panel, so the modules are emitted here instead and coloured from the theme. Scanners need
 * light modules on a dark-enough background *and* a quiet zone, so the code keeps a white
 * plate and a 2-module margin rather than going fully transparent — a QR that matches the
 * theme but doesn't scan is decoration.
 */
export function QrCode({ value, size = 148 }: Props) {
  const { path, dimension } = useMemo(() => {
    // Type 0 = pick the smallest version that fits; "M" survives a bit of screen glare.
    const qr = qrcode(0, "M");
    qr.addData(value);
    qr.make();

    const count = qr.getModuleCount();
    const margin = 2;
    const commands: string[] = [];
    for (let row = 0; row < count; row++) {
      for (let col = 0; col < count; col++) {
        if (qr.isDark(row, col)) {
          commands.push(`M${col + margin} ${row + margin}h1v1h-1z`);
        }
      }
    }
    return { path: commands.join(""), dimension: count + margin * 2 };
  }, [value]);

  return (
    <svg
      className="qr-code"
      width={size}
      height={size}
      viewBox={`0 0 ${dimension} ${dimension}`}
      shapeRendering="crispEdges"
      role="img"
      aria-label="Referral link QR code"
    >
      <rect width={dimension} height={dimension} fill="#ffffff" />
      <path d={path} fill="#000000" />
    </svg>
  );
}
